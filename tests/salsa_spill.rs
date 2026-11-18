//! The values of persisted memos can be given to a store and read back when something asks, without
//! the query that made them running again, and a memo that was spilled is still invalidated by what
//! it read.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use salsa::{Database, Setter, SpillKey, SpillStore, Storage};

#[salsa::input(persist, singleton)]
struct Source {
    #[returns(copy)]
    value: usize,
}

#[salsa::interned(persist)]
struct Key<'db> {
    n: usize,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

static EXECUTED: AtomicUsize = AtomicUsize::new(0);

/// A persisted query whose value is bigger than a number, so that what comes back is the value.
#[salsa::tracked(returns(ref), persist)]
fn described<'db>(db: &'db dyn Database, key: Key<'db>) -> String {
    EXECUTED.fetch_add(1, Ordering::SeqCst);
    format!("{}:{}", key.n(db), Source::get(db).value(db))
}

/// A store in memory, which counts what is asked of it.
#[derive(Default)]
struct Counting {
    held: Mutex<HashMap<SpillKey, Vec<u8>>>,
    puts: AtomicUsize,
    gets: AtomicUsize,
}

impl SpillStore for Counting {
    fn put(&self, key: SpillKey, bytes: &[u8]) {
        self.puts.fetch_add(1, Ordering::SeqCst);
        self.held.lock().unwrap().insert(key, bytes.to_vec());
    }

    fn get(&self, key: SpillKey) -> Option<Vec<u8>> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        self.held.lock().unwrap().get(&key).cloned()
    }
}

const KEYS: usize = 20;

/// Whether the values are spilled between the first read and the second: the one thing a test changes.
#[derive(Clone, Copy)]
enum Between {
    Spill,
    Nothing,
}

/// A database with `KEYS` values read once, then `between` done, then read again; what was executed
/// and asked of the store by the second read.
fn read_twice(between: Between) -> (usize, usize, usize, Vec<String>, Db, Arc<Counting>) {
    let store = Arc::new(Counting::default());
    salsa::set_spill_store(Some(store.clone()));
    let mut db = Db::default();
    Source::new(&db, 1);
    let read = |db: &Db| -> Vec<String> {
        (0..KEYS)
            .map(|n| described(db, Key::new(db, n)).clone())
            .collect()
    };
    read(&db);
    let (before, puts_before) = (
        EXECUTED.load(Ordering::SeqCst),
        store.puts.load(Ordering::SeqCst),
    );
    if matches!(between, Between::Spill) {
        let report = db.spill_persisted();
        assert_eq!(report.written, KEYS as u64, "every value was written");
    }
    let values = read(&db);
    let executed = EXECUTED.load(Ordering::SeqCst) - before;
    let gets = store.gets.load(Ordering::SeqCst);
    (executed, puts_before, gets, values, db, store)
}

/// Spilled values come back from the store, and the queries that made them do not run again.
#[test]
fn a_spilled_value_is_read_back_and_its_query_does_not_run_again() {
    let (executed, _, gets, values, _, _) = read_twice(Between::Spill);
    assert_eq!(executed, 0, "no query ran again");
    assert_eq!(gets, KEYS, "each value came from the store");
    assert_eq!(values[7], "7:1");
}

/// Without the spill nothing is asked of the store: what shows the reads above are the spill's.
#[test]
fn without_a_spill_the_store_is_never_asked() {
    let (executed, puts, gets, _, _, _) = read_twice(Between::Nothing);
    assert_eq!((executed, puts, gets), (0, 0, 0));
}

/// A value read back and spilled again is not written again, and an edit to what a spilled memo
/// read makes its query run again and give the new value.
#[test]
fn a_value_read_back_is_not_written_again_and_an_edit_still_reaches_a_spilled_memo() {
    let (_, _, _, _, mut db, store) = read_twice(Between::Spill);
    let report = db.spill_persisted();
    assert_eq!(
        (report.written, report.dropped_again),
        (0, KEYS as u64),
        "the store already held what was read back"
    );
    assert_eq!(store.puts.load(Ordering::SeqCst), KEYS, "written once each");

    let before = EXECUTED.load(Ordering::SeqCst);
    Source::get(&db).set_value(&mut db).to(2);
    let value = described(&db, Key::new(&db, 3)).clone();
    assert_eq!(value, "3:2", "the edit reached the spilled memo");
    assert_eq!(
        EXECUTED.load(Ordering::SeqCst) - before,
        1,
        "and only its query ran"
    );
}
