//! Owner links follow actual RAM representations, including delayed and encoded payloads.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use salsa::plumbing::{AsId, FromId, ZalsaDatabase};
use salsa::{Database, Id, QueryOwnership, Setter, SpillKey, SpillStore, Storage};

static FIXTURE: Mutex<()> = Mutex::new(());
static EXECUTIONS: AtomicUsize = AtomicUsize::new(0);
static CLASSIFICATIONS: AtomicUsize = AtomicUsize::new(0);
static DECODES: AtomicUsize = AtomicUsize::new(0);

#[salsa::input(persist)]
struct Source {
    owners: Vec<Id>,
    #[returns(copy)]
    value: u8,
}

#[salsa::interned(persist)]
struct Arguments<'db> {
    source: Source,
    offset: u8,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, salsa::SalsaValue)]
struct Payload {
    #[serde(serialize_with = "encode_bytes", deserialize_with = "decode_bytes")]
    bytes: Vec<u8>,
    #[serde(skip)]
    alive: Arc<()>,
}

fn encode_bytes<S: serde::Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    if bytes.first() == Some(&255) {
        return Err(serde::ser::Error::custom("fixture rejects encoding"));
    }
    serde::Serialize::serialize(bytes, serializer)
}

fn decode_bytes<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    DECODES.fetch_add(1, Ordering::SeqCst);
    serde::Deserialize::deserialize(deserializer)
}

fn payload(db: &dyn Database, arguments: Arguments<'_>) -> Payload {
    EXECUTIONS.fetch_add(1, Ordering::SeqCst);
    Payload {
        bytes: vec![
            arguments
                .source(db)
                .value(db)
                .wrapping_add(*arguments.offset(db));
            4096
        ],
        alive: Arc::new(()),
    }
}

#[salsa::tracked(returns(ref))]
fn transient(db: &dyn Database, arguments: Arguments<'_>) -> Payload {
    payload(db, arguments)
}

#[salsa::tracked(persist, returns(ref))]
fn persisted(db: &dyn Database, arguments: Arguments<'_>) -> Payload {
    payload(db, arguments)
}

#[salsa::tracked(returns(ref))]
fn volatile(db: &dyn Database, arguments: Arguments<'_>) -> Payload {
    db.report_untracked_read();
    payload(db, arguments)
}

fn owners(db: &dyn Database, id: Id) -> QueryOwnership {
    CLASSIFICATIONS.fetch_add(1, Ordering::SeqCst);
    QueryOwnership::owned(Arguments::from_id(id).source(db).owners(db).iter().copied())
}

fn register(db: &mut Db) {
    salsa::register_query_ownership::<transient, dyn Database>(db, owners);
    salsa::register_query_ownership::<persisted, dyn Database>(db, owners);
    salsa::register_query_ownership::<volatile, dyn Database>(db, owners);
}

fn fixture() -> (Db, Id, Source, Id) {
    let mut db = Db::default();
    register(&mut db);
    let root = Source::new(&db, vec![], 0).as_id();
    salsa::activate_owner_roots(&mut db, &[root]);
    let source = Source::new(&db, vec![root], 7);
    let key = Arguments::new(&db, source, 0).as_id();
    (db, root, source, key)
}

#[derive(Default)]
struct Store {
    bytes: Mutex<HashMap<SpillKey, Vec<u8>>>,
    panic_put: AtomicBool,
}

impl SpillStore for Store {
    fn put(&self, key: SpillKey, bytes: &[u8]) {
        assert!(
            !self.panic_put.load(Ordering::SeqCst),
            "fixture store panics"
        );
        self.bytes.lock().unwrap().insert(key, bytes.to_vec());
    }

    fn get(&self, key: SpillKey) -> Option<Vec<u8>> {
        self.bytes.lock().unwrap().get(&key).cloned()
    }
}

struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        salsa::set_spill_store(None);
    }
}

#[test]
fn replacement_releases_deferred_allocations_at_the_exclusive_boundary() {
    let _fixture = FIXTURE.lock().unwrap();
    let (mut db, root, source, key) = fixture();
    let old = Arc::downgrade(&transient(&db, Arguments::from_id(key)).alive);
    source.set_value(&mut db).to(8);
    let current = Arc::downgrade(&transient(&db, Arguments::from_id(key)).alive);
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 1);
    assert_eq!(
        old.strong_count(),
        1,
        "replaced allocation is safely deferred"
    );
    let before = (
        EXECUTIONS.load(Ordering::SeqCst),
        CLASSIFICATIONS.load(Ordering::SeqCst),
    );
    let report = salsa::retire_owned_values(&mut db, &[root]);
    assert_eq!((report.released, report.deferred_released), (1, 1));
    assert_eq!((old.strong_count(), current.strong_count()), (0, 0));
    assert_eq!(
        (
            EXECUTIONS.load(Ordering::SeqCst),
            CLASSIFICATIONS.load(Ordering::SeqCst)
        ),
        before
    );
    assert_eq!(salsa::owner_index_stats(&mut db).capacity, 0);
}

#[test]
fn spill_fetch_and_restore_track_encoded_and_decoded_ram_without_reclassification() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = Reset;
    let store = Arc::new(Store::default());
    salsa::set_spill_store(Some(store));
    let (mut db, root, _, key) = fixture();
    let original = Arc::downgrade(&persisted(&db, Arguments::from_id(key)).alive);
    let before = (
        EXECUTIONS.load(Ordering::SeqCst),
        CLASSIFICATIONS.load(Ordering::SeqCst),
        DECODES.load(Ordering::SeqCst),
    );
    let report = salsa::retire_owned_values(&mut db, &[root]);
    assert_eq!((report.released, report.spill.written), (1, 1));
    assert_eq!(original.strong_count(), 0);
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 0);
    salsa::activate_owner_roots(&mut db, &[root]);
    assert_eq!(persisted(&db, Arguments::from_id(key)).bytes, vec![7; 4096]);
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 1);
    assert_eq!(
        (
            EXECUTIONS.load(Ordering::SeqCst),
            CLASSIFICATIONS.load(Ordering::SeqCst)
        ),
        (before.0, before.1)
    );
    assert_eq!(DECODES.load(Ordering::SeqCst), before.2 + 1);
    let encoded = rmp_serde::to_vec(&<dyn Database>::as_serialize(&mut db)).unwrap();
    let before_decode = DECODES.load(Ordering::SeqCst);
    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut rmp_serde::Deserializer::new(encoded.as_slice()),
    )
    .unwrap();
    register(&mut restored);
    assert_eq!(
        salsa::owner_index_stats(&mut restored).memberships,
        1,
        "held bytes are indexed before decode"
    );
    assert_eq!(DECODES.load(Ordering::SeqCst), before_decode);
    let report = salsa::retire_owned_values(&mut restored, &[root]);
    assert_eq!((report.released, report.spill.written), (1, 1));
    assert_eq!(
        DECODES.load(Ordering::SeqCst),
        before_decode,
        "retirement never decodes"
    );
    assert_eq!(salsa::owner_index_stats(&mut restored).capacity, 0);
    salsa::activate_owner_roots(&mut restored, &[root]);
    assert_eq!(
        persisted(&restored, Arguments::from_id(key)).bytes,
        vec![7; 4096]
    );
    assert_eq!(salsa::owner_index_stats(&mut restored).memberships, 1);
    assert_eq!(
        (
            EXECUTIONS.load(Ordering::SeqCst),
            CLASSIFICATIONS.load(Ordering::SeqCst)
        ),
        (before.0, before.1)
    );
}

#[test]
fn unavailable_failed_and_panicking_spills_preserve_payloads_and_retry_links() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = Reset;
    salsa::set_spill_store(None);
    let (mut db, root, source, key) = fixture();
    let alive = Arc::downgrade(&persisted(&db, Arguments::from_id(key)).alive);
    assert_eq!(salsa::retire_owned_values(&mut db, &[root]).retained, 1);
    assert_eq!(alive.strong_count(), 1);
    let store = Arc::new(Store::default());
    store.panic_put.store(true, Ordering::SeqCst);
    salsa::set_spill_store(Some(store.clone()));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| salsa::retire_owned_values(
            &mut db,
            &[root]
        )))
        .is_err()
    );
    assert_eq!(
        alive.strong_count(),
        1,
        "store panic preserves the computed value"
    );
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 1);
    store.panic_put.store(false, Ordering::SeqCst);
    assert_eq!(salsa::retire_owned_values(&mut db, &[root]).released, 1);
    assert_eq!(alive.strong_count(), 0);
    salsa::activate_owner_roots(&mut db, &[root]);
    source.set_value(&mut db).to(255);
    let failed = Arc::downgrade(&persisted(&db, Arguments::from_id(key)).alive);
    assert_eq!(salsa::retire_owned_values(&mut db, &[root]).retained, 1);
    assert_eq!(failed.strong_count(), 1);
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 1);
}

#[test]
fn non_recomputable_origins_remain_indexed_and_explicit_drop_detaches_eligible_memos() {
    let _fixture = FIXTURE.lock().unwrap();
    let (mut db, root, _, key) = fixture();
    let protected = Arc::downgrade(&volatile(&db, Arguments::from_id(key)).alive);
    transient(&db, Arguments::from_id(key));
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 2);
    db.drop_regenerable_values();
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 1);
    let report = salsa::retire_owned_values(&mut db, &[root]);
    assert_eq!(
        (report.selected, report.retained, report.released),
        (1, 1, 0)
    );
    assert_eq!(protected.strong_count(), 1);
}

#[salsa::tracked(returns(copy), cycle_initial = cycle_initial)]
fn cycle(db: &dyn Database, arguments: Arguments<'_>) -> u8 {
    cycle(db, arguments).max(arguments.source(db).value(db))
}

fn cycle_initial(_db: &dyn Database, _id: Id, _arguments: Arguments<'_>) -> u8 {
    0
}

#[salsa::tracked(returns(ref), lru = 1)]
fn limited(db: &dyn Database, arguments: Arguments<'_>) -> Payload {
    payload(db, arguments)
}

#[salsa::tracked(returns(ref))]
fn interruptible(db: &dyn Database, arguments: Arguments<'_>) -> Payload {
    match arguments.source(db).value(db) {
        0 => panic!("fixture body panics"),
        1 => std::panic::resume_unwind(Box::new(salsa::Cancelled::Local)),
        _ => payload(db, arguments),
    }
}

#[test]
fn cycles_lru_panics_and_cancellation_leave_only_current_resident_links() {
    let _fixture = FIXTURE.lock().unwrap();
    let (mut db, root, source, key) = fixture();
    salsa::register_query_ownership::<cycle, dyn Database>(&mut db, owners);
    salsa::register_query_ownership::<limited, dyn Database>(&mut db, owners);
    salsa::register_query_ownership::<interruptible, dyn Database>(&mut db, owners);
    assert_eq!(cycle(&db, Arguments::from_id(key)), 7);
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 1);
    let report = salsa::retire_owned_values(&mut db, &[root]);
    assert_eq!(
        (report.selected, report.released, report.retained),
        (1, 1, 0)
    );
    salsa::activate_owner_roots(&mut db, &[root]);
    for value in [0, 1] {
        source.set_value(&mut db).to(value);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            interruptible(&db, Arguments::from_id(key))
        }));
        assert!(result.is_err());
        assert_eq!(salsa::owner_index_stats(&mut db).memberships, 0);
    }
    source.set_value(&mut db).to(7);
    interruptible(&db, Arguments::from_id(key));
    for offset in 0..10 {
        limited(&db, Arguments::new(&db, source, offset));
    }
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 11);
    db.trigger_lru_eviction();
    let stats = salsa::owner_index_stats(&mut db);
    assert!(stats.memberships < 11, "LRU must detach evicted memos");
    assert_eq!(
        salsa::retire_owned_values(&mut db, &[root]).released,
        stats.memberships
    );
    assert_eq!(salsa::owner_index_stats(&mut db).capacity, 0);
}

#[test]
fn parallel_publication_has_one_link_per_exact_memo() {
    let _fixture = FIXTURE.lock().unwrap();
    let (mut db, root, source, _) = fixture();
    let handles: Vec<_> = (0..4).map(|_| db.clone()).collect();
    std::thread::scope(|scope| {
        for handle in handles {
            scope.spawn(move || {
                for offset in 0..16 {
                    transient(&handle, Arguments::new(&handle, source, offset));
                }
            });
        }
    });
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 16);
    let report = salsa::retire_owned_values(&mut db, &[root]);
    assert_eq!(
        (report.visited, report.selected, report.released),
        (16, 16, 16)
    );
    assert_eq!(salsa::owner_index_stats(&mut db).capacity, 0);
}

#[test]
fn decoder_panic_does_not_attach_a_disk_only_memo() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = Reset;
    let store = Arc::new(Store::default());
    salsa::set_spill_store(Some(store.clone()));
    let (mut db, root, _, key) = fixture();
    persisted(&db, Arguments::from_id(key));
    assert_eq!(salsa::retire_owned_values(&mut db, &[root]).released, 1);
    salsa::activate_owner_roots(&mut db, &[root]);
    let original = store.bytes.lock().unwrap().clone();
    for bytes in store.bytes.lock().unwrap().values_mut() {
        bytes.clear();
    }
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| persisted(
            &db,
            Arguments::from_id(key)
        )))
        .is_err()
    );
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 0);
    *store.bytes.lock().unwrap() = original;
    assert_eq!(persisted(&db, Arguments::from_id(key)).bytes, vec![7; 4096]);
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 1);
}

#[salsa::interned(revisions = 1)]
struct Reusable<'db> {
    source: Source,
    number: u32,
}

#[salsa::tracked(returns(ref))]
fn keys(db: &dyn Database, source: Source) -> Vec<Reusable<'_>> {
    let revision = u32::from(source.value(db));
    (0..1024)
        .map(|number| Reusable::new(db, source, revision * 1024 + number))
        .collect()
}

#[salsa::tracked(returns(ref))]
fn reused_payload(db: &dyn Database, key: Reusable<'_>) -> Payload {
    Payload {
        bytes: vec![*key.number(db) as u8; 128],
        alive: Arc::new(()),
    }
}

fn reused_owners(db: &dyn Database, id: Id) -> QueryOwnership {
    QueryOwnership::owned(Reusable::from_id(id).source(db).owners(db).iter().copied())
}

#[test]
fn slot_reuse_detaches_old_generations_before_indexing_new_memos() {
    let _fixture = FIXTURE.lock().unwrap();
    let (mut db, root, source, _) = fixture();
    salsa::register_query_ownership::<reused_payload, dyn Database>(&mut db, reused_owners);
    let mut markers = Vec::new();
    let mut reused = false;
    for revision in 0..5 {
        source.set_value(&mut db).to(revision);
        for key in keys(&db, source) {
            reused |= key.as_id().generation() > 0;
            markers.push(Arc::downgrade(&reused_payload(&db, *key).alive));
        }
    }
    assert!(reused, "fixture must exercise real slot reclamation");
    let live = markers
        .iter()
        .filter(|marker| marker.strong_count() > 0)
        .count();
    assert!(live < markers.len());
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, live);
    assert_eq!(salsa::retire_owned_values(&mut db, &[root]).released, live);
    assert!(markers.iter().all(|marker| marker.strong_count() == 0));
    assert_eq!(salsa::owner_index_stats(&mut db).capacity, 0);
}

#[salsa::tracked]
struct Assigned<'db> {
    source: Source,
}

#[salsa::tracked(specify, returns(ref))]
fn assigned(_db: &dyn Database, _key: Assigned<'_>) -> Payload {
    unreachable!("fixture always specifies its value")
}

#[salsa::tracked(returns(copy))]
fn assign(db: &dyn Database, source: Source) -> Assigned<'_> {
    let key = Assigned::new(db, source);
    assigned::specify(
        db,
        key,
        Payload {
            bytes: vec![source.value(db); 128],
            alive: Arc::new(()),
        },
    );
    key
}

fn assigned_owners(db: &dyn Database, id: Id) -> QueryOwnership {
    QueryOwnership::owned(Assigned::from_id(id).source(db).owners(db).iter().copied())
}

#[test]
fn assigned_values_refuse_retirement_without_changing_revision() {
    let _fixture = FIXTURE.lock().unwrap();
    let (mut db, root, source, _) = fixture();
    salsa::register_query_ownership::<assigned, dyn Database>(&mut db, assigned_owners);
    let alive = Arc::downgrade(&assigned(&db, assign(&db, source)).alive);
    let revision = db.zalsa().current_revision();
    let report = salsa::retire_owned_values(&mut db, &[root]);
    assert_eq!(
        (report.selected, report.retained, report.refused),
        (1, 1, 1)
    );
    assert_eq!(db.zalsa().current_revision(), revision);
    assert_eq!(alive.strong_count(), 1);
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 1);
    salsa::activate_owner_roots(&mut db, &[root]);
    assert_eq!(db.zalsa().current_revision(), revision);
}

#[test]
fn incompatible_snapshot_versions_fail_before_loading_memos() {
    let _fixture = FIXTURE.lock().unwrap();
    // Versions 1 and 2 are the two ways of writing stamps (see `stamps`); any other is refused.
    for encoded in ["[0]", "[3]", "{\"ingredients\":[]}"] {
        let mut db = Db::default();
        let error =
            <dyn Database>::deserialize(&mut db, &mut serde_json::Deserializer::from_str(encoded))
                .unwrap_err();
        assert!(
            error.to_string().contains("ownership format") || error.to_string().contains("format"),
            "{error}"
        );
        assert_eq!(salsa::owner_index_stats(&mut db), Default::default());
    }
}

static STARTED: Mutex<Option<std::sync::mpsc::SyncSender<()>>> = Mutex::new(None);

#[salsa::tracked(returns(ref))]
fn cancellable(db: &dyn Database, arguments: Arguments<'_>) -> Payload {
    let started = STARTED.lock().unwrap().take();
    if let Some(started) = started {
        started.send(()).unwrap();
        loop {
            db.unwind_if_revision_cancelled();
            std::thread::yield_now();
        }
    }
    payload(db, arguments)
}

#[test]
fn retirement_cancels_an_inflight_query_before_mutating_storage() {
    let _fixture = FIXTURE.lock().unwrap();
    let (mut db, root, _, key) = fixture();
    salsa::register_query_ownership::<cancellable, dyn Database>(&mut db, owners);
    let (started, wait) = std::sync::mpsc::sync_channel(1);
    *STARTED.lock().unwrap() = Some(started);
    let other = db.clone();
    let worker = std::thread::spawn(move || {
        salsa::Cancelled::catch(|| {
            cancellable(&other, Arguments::from_id(key));
        })
    });
    wait.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    let revision = db.zalsa().current_revision();
    let report = salsa::retire_owned_values(&mut db, &[root]);
    assert!(worker.join().unwrap().is_err());
    assert_eq!((report.selected, report.released), (0, 0));
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 0);
    assert_eq!(db.zalsa().current_revision(), revision);
    salsa::activate_owner_roots(&mut db, &[root]);
    assert_eq!(
        cancellable(&db, Arguments::from_id(key)).bytes,
        vec![7; 4096]
    );
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 1);
    assert_eq!(salsa::retire_owned_values(&mut db, &[root]).released, 1);
}

#[salsa::tracked(returns(copy), cycle_result = fallback)]
fn immediate_fallback(db: &dyn Database, arguments: Arguments<'_>) -> u8 {
    immediate_fallback(db, arguments) + 1
}

fn fallback(_db: &dyn Database, _id: Id, _arguments: Arguments<'_>) -> u8 {
    0
}

#[test]
fn immediate_cycle_fallback_has_one_current_membership() {
    let _fixture = FIXTURE.lock().unwrap();
    let (mut db, root, _, key) = fixture();
    salsa::register_query_ownership::<immediate_fallback, dyn Database>(&mut db, owners);
    assert_eq!(immediate_fallback(&db, Arguments::from_id(key)), 0);
    assert_eq!(salsa::owner_index_stats(&mut db).memberships, 1);
    assert_eq!(salsa::retire_owned_values(&mut db, &[root]).released, 1);
    assert_eq!(salsa::owner_index_stats(&mut db).capacity, 0);
}

#[test]
fn restored_encoded_population_retires_by_owner_without_decoding_or_scanning_neighbors() {
    let _fixture = FIXTURE.lock().unwrap();
    let _reset = Reset;
    salsa::set_spill_store(Some(Arc::new(Store::default())));
    for contexts in [1, 4, 16] {
        for shared_count in [1, 32] {
            let mut db = Db::default();
            register(&mut db);
            let roots: Vec<_> = (0..contexts)
                .map(|_| Source::new(&db, vec![], 0).as_id())
                .collect();
            salsa::activate_owner_roots(&mut db, &roots);
            let local_keys: Vec<_> = roots
                .iter()
                .enumerate()
                .map(|(index, &root)| {
                    let source = Source::new(&db, vec![root], index as u8);
                    let key = Arguments::new(&db, source, 0);
                    persisted(&db, key);
                    key.as_id()
                })
                .collect();
            let source = Source::new(&db, vec![], 23);
            let shared_keys: Vec<_> = (0..shared_count)
                .map(|offset| {
                    let key = Arguments::new(&db, source, offset);
                    persisted(&db, key);
                    key.as_id()
                })
                .collect();
            let saved = rmp_serde::to_vec(&<dyn Database>::as_serialize(&mut db)).unwrap();
            let before = (
                DECODES.load(Ordering::SeqCst),
                EXECUTIONS.load(Ordering::SeqCst),
                CLASSIFICATIONS.load(Ordering::SeqCst),
            );
            let mut restored = Db::default();
            <dyn Database>::deserialize(
                &mut restored,
                &mut rmp_serde::Deserializer::new(saved.as_slice()),
            )
            .unwrap();
            register(&mut restored);
            assert_eq!(
                salsa::owner_index_stats(&mut restored).memberships,
                contexts
            );
            for (index, &root) in roots.iter().enumerate() {
                let report = salsa::retire_owned_values(&mut restored, &[root]);
                assert_eq!(
                    (
                        report.visited,
                        report.selected,
                        report.released,
                        report.spill.written
                    ),
                    (1, 1, 1, 1)
                );
                let stats = salsa::owner_index_stats(&mut restored);
                assert_eq!(
                    (stats.memberships, stats.shared),
                    (contexts - index - 1, shared_count as usize)
                );
            }
            assert_eq!(salsa::owner_index_stats(&mut restored).capacity, 0);
            assert_eq!(
                (
                    DECODES.load(Ordering::SeqCst),
                    EXECUTIONS.load(Ordering::SeqCst),
                    CLASSIFICATIONS.load(Ordering::SeqCst)
                ),
                before
            );
            for (index, &key) in local_keys.iter().enumerate() {
                salsa::activate_owner_roots(&mut restored, &roots[index..index + 1]);
                assert_eq!(
                    persisted(&restored, Arguments::from_id(key)).bytes,
                    vec![index as u8; 4096]
                );
            }
            for (offset, &key) in shared_keys.iter().enumerate() {
                assert_eq!(
                    persisted(&restored, Arguments::from_id(key)).bytes,
                    vec![23 + offset as u8; 4096]
                );
            }
            assert_eq!(
                (
                    EXECUTIONS.load(Ordering::SeqCst),
                    CLASSIFICATIONS.load(Ordering::SeqCst)
                ),
                (before.1, before.2)
            );
        }
    }
}
