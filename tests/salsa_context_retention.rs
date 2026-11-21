//! Context payloads stored in tracked structs outlive query memo eviction and spilling.
//! rust-analyzer's `DefMapPair` uses this layout. Weak references check ownership of the
//! original payloads, not heap bytes or process RSS; the store here keeps encoded test data.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use salsa::{Database, SpillKey, SpillStore, Storage};

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

/// The shape used by RA's `DefMapPair`: identity fields and a tracked semantic payload.
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
