//! A restored memo keeps its value as the bytes it was saved as until a query reads it: restoring
//! decodes nothing, and what is saved again without having been read is saved as the bytes it was.

use std::sync::atomic::{AtomicUsize, Ordering};

use salsa::{Database, Setter, Storage};

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

static DECODED: AtomicUsize = AtomicUsize::new(0);
static ENCODED: AtomicUsize = AtomicUsize::new(0);
static EXECUTED: AtomicUsize = AtomicUsize::new(0);

/// A value that says when it is encoded and when it is decoded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::SalsaValue)]
struct Counted(String);

impl serde::Serialize for Counted {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ENCODED.fetch_add(1, Ordering::SeqCst);
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for Counted {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        DECODED.fetch_add(1, Ordering::SeqCst);
        String::deserialize(deserializer).map(Counted)
    }
}

#[salsa::tracked(returns(ref), persist)]
fn described<'db>(db: &'db dyn Database, key: Key<'db>) -> Counted {
    EXECUTED.fetch_add(1, Ordering::SeqCst);
    Counted(format!("{}:{}", key.n(db), Source::get(db).value(db)))
}

const KEYS: usize = 25;

fn read(db: &Db, count: usize) -> Vec<String> {
    (0..count)
        .map(|n| described(db, Key::new(db, n)).0.clone())
        .collect()
}

fn save(db: &mut Db) -> String {
    serde_json::to_string(&<dyn Database>::as_serialize(db))
        .unwrap_or_else(|error| panic!("serializing: {error}"))
}

fn restore(saved: &str) -> Db {
    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(saved),
    )
    .unwrap_or_else(|error| panic!("deserializing: {error}"));
    restored
}

/// Everything a test needs to know of one save, restore, read, save again.
struct Run {
    /// Values decoded by the restore itself.
    decoded_by_restore: usize,
    /// Values decoded after reading `read_count` of them.
    decoded_after_reading: usize,
    /// Values encoded by saving the restored database again.
    encoded_by_second_save: usize,
    /// How many queries ran again.
    executed: usize,
    values: Vec<String>,
    second: String,
    first: String,
}

fn run(read_count: usize) -> Run {
    let mut db = Db::default();
    Source::new(&db, 1);
    read(&db, KEYS);
    let first = save(&mut db);

    let (decoded, executed) = (
        DECODED.load(Ordering::SeqCst),
        EXECUTED.load(Ordering::SeqCst),
    );
    let mut restored = restore(&first);
    let decoded_by_restore = DECODED.load(Ordering::SeqCst) - decoded;
    let values = read(&restored, read_count);
    let decoded_after_reading = DECODED.load(Ordering::SeqCst) - decoded;
    let encoded = ENCODED.load(Ordering::SeqCst);
    let second = save(&mut restored);
    Run {
        decoded_by_restore,
        decoded_after_reading,
        encoded_by_second_save: ENCODED.load(Ordering::SeqCst) - encoded,
        executed: EXECUTED.load(Ordering::SeqCst) - executed,
        values,
        second,
        first,
    }
}

/// Restoring decodes no value; a value is decoded when a query reads it, and no query runs again.
#[test]
fn a_restore_decodes_nothing_and_a_read_decodes_what_it_reads() {
    let run = run(5);
    assert_eq!(run.decoded_by_restore, 0, "restoring decoded values");
    assert_eq!(run.decoded_after_reading, 5, "only what was read");
    assert_eq!(run.executed, 0, "no query ran again");
    assert_eq!(run.values[3], "3:1");
}

/// A restored value is saved again as the bytes it was, read or not: it cannot have changed, so
/// nothing is encoded, and the same state comes out.
#[test]
fn a_restored_value_is_saved_again_as_the_bytes_it_was() {
    let run = run(5);
    assert_eq!(
        run.encoded_by_second_save, 0,
        "no value is encoded again, those that were read included"
    );
    assert_eq!(run.second, run.first, "and the state is the same");
}

/// An edit reaches a memo whose value was never decoded.
#[test]
fn an_edit_reaches_a_value_that_was_never_decoded() {
    let mut db = Db::default();
    let source = Source::new(&db, 1);
    read(&db, KEYS);
    let saved = save(&mut db);
    let mut restored = restore(&saved);
    let before = EXECUTED.load(Ordering::SeqCst);
    Source::get(&restored).set_value(&mut restored).to(2);
    let _ = source;
    assert_eq!(
        read(&restored, 3)[1],
        "1:2",
        "the edit reached the saved memo"
    );
    assert_eq!(
        EXECUTED.load(Ordering::SeqCst) - before,
        3,
        "and only what was read ran"
    );
}
