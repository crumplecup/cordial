//! What a database holds in memory can be counted by query, and dropped one query at a time: the
//! ledger of what each kind of cache is made of is measured by dropping a query's values and seeing
//! what the process gives back. A memo whose value is dropped is computed again when something reads
//! it; one that was restored from a saved state decodes its bytes again and computes nothing.

use std::sync::atomic::{AtomicUsize, Ordering};

use salsa::{Database, Storage};

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

static DESCRIBED: AtomicUsize = AtomicUsize::new(0);
static PLAIN: AtomicUsize = AtomicUsize::new(0);
static DECODED: AtomicUsize = AtomicUsize::new(0);

/// A value that says when it is decoded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::SalsaValue)]
struct Counted(String);

impl serde::Serialize for Counted {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for Counted {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        DECODED.fetch_add(1, Ordering::SeqCst);
        String::deserialize(deserializer).map(Counted)
    }
}

/// A persisted query.
#[salsa::tracked(returns(ref), persist)]
fn described<'db>(db: &'db dyn Database, key: Key<'db>) -> Counted {
    DESCRIBED.fetch_add(1, Ordering::SeqCst);
    Counted(format!("{}:{}", key.n(db), Source::get(db).value(db)))
}

/// A query that is not persisted.
#[salsa::tracked(returns(copy))]
fn plain<'db>(db: &'db dyn Database, key: Key<'db>) -> usize {
    PLAIN.fetch_add(1, Ordering::SeqCst);
    key.n(db) + Source::get(db).value(db)
}

const DESCRIBED_KEYS: usize = 10;
const PLAIN_KEYS: usize = 7;

fn read(db: &Db) {
    for n in 0..DESCRIBED_KEYS {
        let _ = described(db, Key::new(db, n));
    }
    for n in 0..PLAIN_KEYS {
        let _ = plain(db, Key::new(db, 100 + n));
    }
}

/// How many memos of each query hold a value in memory, and the drop of the queries `only` names.
fn held(db: &mut Db, drop: bool, only: &[&str]) -> Vec<(&'static str, u64)> {
    let mut found = Vec::new();
    db.values_in_memory(drop, true, only, &mut |query, count| {
        found.push((query, count))
    });
    found
}

fn count(found: &[(&str, u64)], query: &str) -> u64 {
    found
        .iter()
        .find(|(name, _)| *name == query)
        .map_or(0, |(_, count)| *count)
}

/// Each query's memos are counted, and counting drops nothing.
#[test]
fn what_is_held_is_counted_by_query() {
    let mut db = Db::default();
    Source::new(&db, 1);
    read(&db);
    let found = held(&mut db, false, &[]);
    assert_eq!(count(&found, "described"), DESCRIBED_KEYS as u64);
    assert_eq!(count(&found, "plain"), PLAIN_KEYS as u64);
    let again = held(&mut db, false, &[]);
    assert_eq!(found, again, "counting is not dropping");
}

/// Dropping one query's values leaves the others, and the dropped are computed again on a read.
#[test]
fn one_query_is_dropped_at_a_time_and_computed_again_when_read() {
    let mut db = Db::default();
    Source::new(&db, 1);
    read(&db);

    let dropped = held(&mut db, true, &["described"]);
    assert_eq!(dropped, vec![("described", DESCRIBED_KEYS as u64)]);
    let left = held(&mut db, false, &[]);
    assert_eq!(count(&left, "described"), 0, "its values are gone");
    assert_eq!(
        count(&left, "plain"),
        PLAIN_KEYS as u64,
        "and the other query's are not"
    );

    let (described_before, plain_before) = (
        DESCRIBED.load(Ordering::SeqCst),
        PLAIN.load(Ordering::SeqCst),
    );
    read(&db);
    assert_eq!(
        DESCRIBED.load(Ordering::SeqCst) - described_before,
        DESCRIBED_KEYS,
        "what was dropped is computed again"
    );
    assert_eq!(
        PLAIN.load(Ordering::SeqCst) - plain_before,
        0,
        "and what was not is not"
    );
}

/// What a restored memo holds as a value is what was decoded: dropping it decodes it again, and
/// computes nothing, since the bytes are still held.
#[test]
fn a_restored_memo_decodes_again_after_a_drop_and_computes_nothing() {
    let mut db = Db::default();
    Source::new(&db, 1);
    read(&db);
    let saved = serde_json::to_string(&<dyn Database>::as_serialize(&mut db))
        .unwrap_or_else(|error| panic!("serializing: {error}"));
    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&saved),
    )
    .unwrap_or_else(|error| panic!("deserializing: {error}"));

    assert_eq!(
        count(&held(&mut restored, false, &[]), "described"),
        0,
        "nothing restored has been decoded"
    );
    let (described_before, decoded_before) = (
        DESCRIBED.load(Ordering::SeqCst),
        DECODED.load(Ordering::SeqCst),
    );
    read(&restored);
    assert_eq!(
        DECODED.load(Ordering::SeqCst) - decoded_before,
        DESCRIBED_KEYS
    );
    assert_eq!(
        count(&held(&mut restored, false, &[]), "described"),
        DESCRIBED_KEYS as u64
    );

    held(&mut restored, true, &["described"]);
    assert_eq!(count(&held(&mut restored, false, &[]), "described"), 0);
    read(&restored);
    assert_eq!(
        DECODED.load(Ordering::SeqCst) - decoded_before,
        2 * DESCRIBED_KEYS,
        "decoded again"
    );
    assert_eq!(
        DESCRIBED.load(Ordering::SeqCst) - described_before,
        0,
        "and nothing was computed"
    );
}

/// What is dropped between phases of a run is what a query computes again and a saved state does
/// not hold: a persisted memo with no value would be missing from the state the run saves.
#[test]
fn the_regenerable_values_are_dropped_and_the_persisted_ones_are_kept_for_the_saved_state() {
    let mut db = Db::default();
    Source::new(&db, 1);
    read(&db);

    assert_eq!(
        db.drop_regenerable_values(),
        PLAIN_KEYS as u64,
        "only the query that is not persisted was dropped"
    );
    let left = held(&mut db, false, &[]);
    assert_eq!(count(&left, "plain"), 0);
    assert_eq!(
        count(&left, "described"),
        DESCRIBED_KEYS as u64,
        "the persisted values stay"
    );

    // The state saved now has every persisted memo, and a restore computes none of them again.
    let saved = serde_json::to_string(&<dyn Database>::as_serialize(&mut db))
        .unwrap_or_else(|error| panic!("serializing: {error}"));
    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&saved),
    )
    .unwrap_or_else(|error| panic!("deserializing: {error}"));
    let before = DESCRIBED.load(Ordering::SeqCst);
    for n in 0..DESCRIBED_KEYS {
        let _ = described(&restored, Key::new(&restored, n));
    }
    assert_eq!(
        DESCRIBED.load(Ordering::SeqCst) - before,
        0,
        "the persisted memos were in the state"
    );
}
