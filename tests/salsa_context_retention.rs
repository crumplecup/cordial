//! Compare RA's tracked-struct map ownership with query-owned maps and independent projections.
//! Weak references check original payload ownership, not heap bytes or process RSS; the store
//! here keeps encoded test data. These are storage fixtures, not selective context eviction.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use salsa::plumbing::AsId;
use salsa::{Database, Setter, SpillKey, SpillStore, Storage};

// The spill store and query execution counters are process-global.
static FIXTURE: Mutex<()> = Mutex::new(());
static OWNERS: AtomicUsize = AtomicUsize::new(0);
static EXTERNAL_READS: AtomicUsize = AtomicUsize::new(0);
static LOCAL_READS: AtomicUsize = AtomicUsize::new(0);

#[salsa::input(persist)]
struct Context {
    #[returns(copy)]
    value: usize,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

/// The lifetime marker is deliberately absent from encoded data. A decoded payload has its
/// own marker, so a weak reference to the original checks whether that original was released.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, salsa::SalsaValue)]
struct Payload {
    bytes: Vec<u8>,
    #[serde(skip)]
    alive: Arc<()>,
}

impl Payload {
    fn new(value: usize) -> Self {
        Self {
            bytes: vec![value as u8; 64 * 1024],
            alive: Arc::new(()),
        }
    }
}

/// RA's former `DefMapPair` shape: identity fields and a tracked semantic payload.
#[salsa::tracked(persist)]
struct Pair<'db> {
    name: usize,
    #[tracked]
    #[returns(ref)]
    map: Payload,
}

#[salsa::tracked(returns(ref), persist)]
fn pair(db: &dyn Database, context: Context) -> Pair<'_> {
    let value = context.value(db);
    Pair::new(db, value, Payload::new(value))
}

/// A direct memo payload serves as the control: spilling it does release the original.
#[salsa::tracked(returns(ref), persist)]
fn direct(db: &dyn Database, context: Context) -> Payload {
    Payload::new(context.value(db))
}

#[salsa::tracked(returns(copy))]
fn read_map(db: &dyn Database, context: Context) -> usize {
    pair(db, context).map(db).bytes.len()
}

#[derive(Default)]
struct EncodedStore(Mutex<HashMap<SpillKey, Vec<u8>>>);

impl SpillStore for EncodedStore {
    fn put(&self, key: SpillKey, bytes: &[u8]) {
        self.0.lock().unwrap().insert(key, bytes.to_vec());
    }

    fn get(&self, key: SpillKey) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
}

struct ResetSpillStore;

impl Drop for ResetSpillStore {
    fn drop(&mut self) {
        salsa::set_spill_store(None);
    }
}

fn live(markers: &[Weak<()>]) -> usize {
    markers
        .iter()
        .filter(|marker| marker.strong_count() > 0)
        .count()
}

/// Even the global query eviction/spill leaves every tracked-struct payload resident.
/// A future context-retirement implementation must release these while preserving re-entry.
#[test]
fn query_eviction_and_spilling_leave_tracked_context_payloads_owned_by_the_database() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetSpillStore;
    for contexts in [1, 4, 16] {
        salsa::set_spill_store(Some(Arc::new(EncodedStore::default())));
        let mut db = Db::default();
        let mut maps = Vec::new();
        let mut memos = Vec::new();
        for value in 0..contexts {
            let context = Context::new(&db, value);
            maps.push(Arc::downgrade(&pair(&db, context).map(&db).alive));
            memos.push(Arc::downgrade(&direct(&db, context).alive));
            assert_eq!(read_map(&db, context), 64 * 1024);

            assert!(db.drop_regenerable_values() > 0);
            let report = db.spill_persisted();
            assert!(report.written > 0);
            assert_eq!(live(&memos), 0, "direct memo payloads were released");
            assert_eq!(live(&maps), value + 1, "tracked maps accumulate");
        }
        drop(db);
        assert_eq!(live(&maps), 0, "the database owned the remaining maps");
    }
}

#[salsa::input(persist)]
struct MapInputs {
    #[returns(copy)]
    name: usize,
    #[returns(copy)]
    external: usize,
    #[returns(copy)]
    local: usize,
}

/// Small tracked identity remains independent of the large query-owned payloads.
#[salsa::tracked(debug, persist)]
struct Module<'db> {
    name: usize,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, salsa::SalsaValue)]
struct Maps<'db> {
    external: Arc<Payload>,
    local: Arc<Payload>,
    module: Module<'db>,
}

#[salsa::tracked(returns(ref), persist)]
fn maps(db: &dyn Database, inputs: MapInputs) -> Maps<'_> {
    OWNERS.fetch_add(1, Ordering::SeqCst);
    Maps {
        external: Arc::new(Payload::new(inputs.external(db))),
        local: Arc::new(Payload::new(inputs.local(db))),
        module: Module::new(db, inputs.name(db)),
    }
}

/// Separate query boundaries let each projection backdate independently of the other.
#[salsa::tracked(returns(ref))]
fn external_map(db: &dyn Database, inputs: MapInputs) -> Arc<Payload> {
    Arc::clone(&maps(db, inputs).external)
}

#[salsa::tracked(returns(ref))]
fn local_map(db: &dyn Database, inputs: MapInputs) -> Arc<Payload> {
    Arc::clone(&maps(db, inputs).local)
}

#[salsa::tracked(returns(copy))]
fn external_consumer(db: &dyn Database, inputs: MapInputs) -> u8 {
    EXTERNAL_READS.fetch_add(1, Ordering::SeqCst);
    external_map(db, inputs).bytes[0]
}

#[salsa::tracked(returns(copy))]
fn local_consumer(db: &dyn Database, inputs: MapInputs) -> u8 {
    LOCAL_READS.fetch_add(1, Ordering::SeqCst);
    local_map(db, inputs).bytes[0]
}

/// Another persisted result can retain module identity without retaining a map.
#[salsa::tracked(returns(copy), persist)]
fn remembered_module(db: &dyn Database, inputs: MapInputs) -> Module<'_> {
    maps(db, inputs).module
}

fn markers(db: &Db, inputs: MapInputs) -> [Weak<()>; 2] {
    let owner = maps(db, inputs);
    assert!(Arc::ptr_eq(&owner.external, external_map(db, inputs)));
    assert!(Arc::ptr_eq(&owner.local, local_map(db, inputs)));
    [
        Arc::downgrade(&owner.external.alive),
        Arc::downgrade(&owner.local.alive),
    ]
}

fn retire(db: &mut Db) {
    db.drop_regenerable_values();
    let report = db.spill_persisted();
    assert!(report.written + report.dropped_again > 0);
}

#[test]
fn query_owned_maps_release_all_payloads_and_reenter_with_the_same_modules() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetSpillStore;
    for contexts in [1, 4, 16] {
        salsa::set_spill_store(Some(Arc::new(EncodedStore::default())));
        let mut db = Db::default();
        let mut originals = Vec::new();
        let mut expected = Vec::new();
        for name in 0..contexts {
            let inputs = MapInputs::new(&db, name, name + 1, name + 2);
            originals.extend(markers(&db, inputs));
            let module = remembered_module(&db, inputs).as_id().as_bits();
            expected.push((inputs, module));
            assert_eq!(external_consumer(&db, inputs), (name + 1) as u8);
            assert_eq!(local_consumer(&db, inputs), (name + 2) as u8);
            retire(&mut db);
            assert_eq!(live(&originals), 0, "completed maps were released");
        }

        let owners = OWNERS.load(Ordering::SeqCst);
        for (name, (inputs, module)) in expected.into_iter().enumerate() {
            let decoded = markers(&db, inputs);
            assert_eq!(remembered_module(&db, inputs).as_id().as_bits(), module);
            assert_eq!(maps(&db, inputs).module.as_id().as_bits(), module);
            assert_eq!(external_consumer(&db, inputs), (name + 1) as u8);
            assert_eq!(local_consumer(&db, inputs), (name + 2) as u8);
            retire(&mut db);
            assert_eq!(live(&decoded), 0, "decoded maps were released again");
        }
        assert_eq!(OWNERS.load(Ordering::SeqCst), owners, "re-entry decoded");
    }
}

#[test]
fn spilling_the_owner_leaves_projection_references_until_they_are_evicted() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetSpillStore;
    salsa::set_spill_store(Some(Arc::new(EncodedStore::default())));
    let mut db = Db::default();
    let inputs = MapInputs::new(&db, 1, 2, 3);
    let originals = markers(&db, inputs);
    assert!(db.spill_persisted().written > 0);
    assert_eq!(live(&originals), 2, "projections still own both maps");
    db.drop_regenerable_values();
    assert_eq!(live(&originals), 0, "the last map references were released");
}

fn save(db: &mut Db) -> String {
    serde_json::to_string(&<dyn Database>::as_serialize(db)).unwrap()
}

fn restore(saved: &str) -> Db {
    let mut db = Db::default();
    <dyn Database>::deserialize(&mut db, &mut serde_json::Deserializer::from_str(saved)).unwrap();
    db
}

#[test]
fn module_identity_survives_owner_recomputation_in_cold_and_restored_databases() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetSpillStore;
    salsa::set_spill_store(None);
    let mut db = Db::default();
    let inputs = MapInputs::new(&db, 1, 2, 3);
    let module = remembered_module(&db, inputs).as_id().as_bits();
    for restored in [false, true] {
        if restored {
            db = restore(&save(&mut db));
        }
        let originals = markers(&db, inputs);
        db.drop_regenerable_values();
        let mut held = 0;
        db.values_in_memory(true, true, &["maps"], &mut |_, count| held += count);
        assert_eq!(
            held, 1,
            "the owning query value was evicted without spilling"
        );
        assert_eq!(live(&originals), 0);

        let owners = OWNERS.load(Ordering::SeqCst);
        if restored {
            // A restored memo still has encoded bytes after dropping its decoded value. An
            // input edit forces execution instead of merely decoding that value again.
            inputs.set_local(&mut db).to(4);
        }
        assert_eq!(maps(&db, inputs).module.as_id().as_bits(), module);
        assert_eq!(remembered_module(&db, inputs).as_id().as_bits(), module);
        assert_eq!(
            OWNERS.load(Ordering::SeqCst),
            owners + 1,
            "the owner executed"
        );
        assert_eq!(external_consumer(&db, inputs), 2);
        assert_eq!(local_consumer(&db, inputs), if restored { 4 } else { 3 });
    }
}

/// Check warm, spilled-owner and saved/restored paths. Consumers/projections stay warm during
/// each edit: evicting their values would itself require their recomputation on the next read.
#[test]
fn independent_projections_preserve_invalidation_and_module_identity() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = ResetSpillStore;
    for restored in [false, true] {
        for spilled in [false, true] {
            salsa::set_spill_store(Some(Arc::new(EncodedStore::default())));
            let mut db = Db::default();
            let inputs = MapInputs::new(&db, 1, 2, 3);
            let module = remembered_module(&db, inputs).as_id().as_bits();
            assert_eq!(external_consumer(&db, inputs), 2);
            assert_eq!(local_consumer(&db, inputs), 3);
            if restored {
                let saved = save(&mut db);
                let owners = OWNERS.load(Ordering::SeqCst);
                db = restore(&saved);
                assert_eq!(maps(&db, inputs).module.as_id().as_bits(), module);
                assert_eq!(remembered_module(&db, inputs).as_id().as_bits(), module);
                assert_eq!(
                    OWNERS.load(Ordering::SeqCst),
                    owners,
                    "restored owner decoded"
                );
                external_consumer(&db, inputs);
                local_consumer(&db, inputs);
            }
            if spilled {
                assert!(db.spill_persisted().written > 0);
            }

            let before = (
                OWNERS.load(Ordering::SeqCst),
                EXTERNAL_READS.load(Ordering::SeqCst),
                LOCAL_READS.load(Ordering::SeqCst),
            );
            inputs.set_local(&mut db).to(4);
            assert_eq!(external_consumer(&db, inputs), 2);
            assert_eq!(local_consumer(&db, inputs), 4);
            assert_eq!(OWNERS.load(Ordering::SeqCst), before.0 + 1);
            assert_eq!(EXTERNAL_READS.load(Ordering::SeqCst), before.1);
            assert_eq!(LOCAL_READS.load(Ordering::SeqCst), before.2 + 1);
            assert_eq!(maps(&db, inputs).module.as_id().as_bits(), module);

            let before = (
                EXTERNAL_READS.load(Ordering::SeqCst),
                LOCAL_READS.load(Ordering::SeqCst),
            );
            inputs.set_external(&mut db).to(5);
            assert_eq!(external_consumer(&db, inputs), 5);
            assert_eq!(local_consumer(&db, inputs), 4);
            assert_eq!(EXTERNAL_READS.load(Ordering::SeqCst), before.0 + 1);
            assert_eq!(LOCAL_READS.load(Ordering::SeqCst), before.1);
            assert_eq!(remembered_module(&db, inputs).as_id().as_bits(), module);
        }
    }
}
