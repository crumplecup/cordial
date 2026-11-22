//! A state saved again after an unrelated change is the same, entry for entry, where nothing
//! changed. What a save writes about a memo, a slot or an input includes the revision it was last
//! verified, interned or changed at, which moves on with every revision: unless those are written
//! so that they do not, every save differs from the one before in every entry, and a save cannot
//! tell what changed (or write only that).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use salsa::{Database, Setter, Storage};

#[salsa::input(persist, singleton)]
struct Left {
    #[returns(copy)]
    value: usize,
}

#[salsa::input(persist, singleton)]
struct Right {
    #[returns(copy)]
    value: usize,
}

#[salsa::interned(persist)]
struct Key<'db> {
    n: usize,
}

/// Made by a query that is not saved, as a module is.
#[salsa::tracked(persist)]
struct Part<'db> {
    #[returns(copy)]
    name: usize,
    #[tracked]
    #[returns(copy)]
    size: usize,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

static EXECUTED: AtomicUsize = AtomicUsize::new(0);

#[salsa::tracked(returns(ref), persist)]
fn from_left<'db>(db: &'db dyn Database, key: Key<'db>) -> String {
    EXECUTED.fetch_add(1, Ordering::SeqCst);
    format!("{}:{}", key.n(db), Left::get(db).value(db))
}

#[salsa::tracked(returns(ref), persist)]
fn from_right<'db>(db: &'db dyn Database, key: Key<'db>) -> String {
    EXECUTED.fetch_add(1, Ordering::SeqCst);
    format!("{}:{}", key.n(db), Right::get(db).value(db))
}

/// Not saved: makes the parts, reading only the left input.
#[salsa::tracked(returns(ref))]
fn parts(db: &dyn Database) -> Vec<Part<'_>> {
    let left = Left::get(db).value(db);
    (0..4).map(|n| Part::new(db, n, left + n)).collect()
}

/// Saved: reads a part's field, so it depends on the left input through a tracked struct.
#[salsa::tracked(persist, returns(copy))]
fn sized(db: &dyn Database) -> usize {
    parts(db).iter().map(|part| part.size(db)).sum()
}

const KEYS: usize = 10;

/// Not saved: interns the keys inside a query, as a query of rust-analyzer interns the ids it makes.
/// A value interned outside a query is immortal, so it is not what a run's interning looks like. The
/// query has no memo in a process that restored the state, so it runs again there and interns again.
#[salsa::tracked(returns(ref))]
fn keys(db: &dyn Database) -> Vec<Key<'_>> {
    (0..KEYS).map(|n| Key::new(db, n)).collect()
}

fn populate(db: &Db) {
    for key in keys(db) {
        from_left(db, *key);
        from_right(db, *key);
    }
    sized(db);
}

/// The state as text, for a restore, and as entries, for a comparison.
fn saved(
    db: &mut Db,
) -> (
    String,
    BTreeMap<String, BTreeMap<String, serde_json::Value>>,
) {
    <dyn Database>::verify_persisted_memos(db);
    let text = serde_json::to_string(&<dyn Database>::as_serialize(db)).expect("serializing");
    let entries = entries_of(&text);
    (text, entries)
}

fn entries_of(text: &str) -> BTreeMap<String, BTreeMap<String, serde_json::Value>> {
    let state: serde_json::Value = serde_json::from_str(text).expect("a state is json");
    let ingredients = match &state {
        serde_json::Value::Object(object) => object["ingredients"].clone(),
        serde_json::Value::Array(array) => array[2].clone(),
        other => panic!("a state is an object or an array: {other}"),
    };
    let serde_json::Value::Object(ingredients) = ingredients else {
        panic!("ingredients are a map");
    };
    ingredients
        .into_iter()
        .map(|(index, entries)| {
            let serde_json::Value::Object(entries) = entries else {
                panic!("an ingredient is a map");
            };
            (index, entries.into_iter().collect())
        })
        .collect()
}

/// The version a state says it was written as: 1 keeps every stamp, 2 writes them all as the first
/// revision.
fn format_of(text: &str) -> u64 {
    let state: serde_json::Value = serde_json::from_str(text).expect("a state is json");
    match &state {
        serde_json::Value::Object(object) => object["format"].as_u64(),
        serde_json::Value::Array(array) => array[0].as_u64(),
        other => panic!("a state is an object or an array: {other}"),
    }
    .expect("a version")
}

fn restore_text(text: &str) -> Db {
    let mut restored = Db::default();
    <dyn Database>::deserialize(&mut restored, &mut serde_json::Deserializer::from_str(text))
        .unwrap_or_else(|error| panic!("deserializing: {error}"));
    restored
}

/// The entries that differ between two saves, by ingredient and key.
fn differing(
    before: &BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    after: &BTreeMap<String, BTreeMap<String, serde_json::Value>>,
) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for (index, entries) in after {
        let old = before.get(index);
        for (key, value) in entries {
            if old.and_then(|old| old.get(key)) != Some(value) {
                found.push((index.clone(), key.clone()));
            }
        }
    }
    for (index, entries) in before {
        for key in entries.keys() {
            if after.get(index).and_then(|after| after.get(key)).is_none() {
                found.push((index.clone(), key.clone()));
            }
        }
    }
    found.sort();
    found
}

/// Writing the right input and reading everything again changes what depends on it: the input, and
/// the memos of the queries that read it. Every other entry, of every kind, is saved as it was.
#[test]
fn a_save_after_an_unrelated_write_differs_only_where_something_changed() {
    let mut db = Db::default();
    Left::new(&db, 1);
    Right::new(&db, 1);
    populate(&db);
    let (first_text, first) = saved(&mut db);
    assert_eq!(
        format_of(&first_text),
        2,
        "every memo was verified: canonical stamps"
    );

    let mut restored = restore_text(&first_text);
    Right::get(&restored).set_value(&mut restored).to(2);
    populate(&restored);
    let (second_text, second) = saved(&mut restored);
    assert_eq!(format_of(&second_text), 2);

    // The right input's slot, and the memo of each `from_right`: nothing else.
    let changed = differing(&first, &second);
    assert_eq!(
        changed.len(),
        1 + KEYS,
        "only the right input and what read it differ: {changed:?}"
    );
}

/// A state saved without verifying its memos still says that they were not valid: a memo that an
/// input changed under is not taken to be current by the run that restores it.
#[test]
fn a_memo_that_was_not_verified_before_the_save_is_not_trusted_after_it() {
    let mut db = Db::default();
    Left::new(&db, 1);
    Right::new(&db, 1);
    populate(&db);
    let (first_text, _) = saved(&mut db);

    let mut restored = restore_text(&first_text);
    Left::get(&restored).set_value(&mut restored).to(2);
    // Saved without verifying: the memos that read the left input are out of date.
    let stale_text =
        serde_json::to_string(&<dyn Database>::as_serialize(&mut restored)).expect("serializing");

    assert_eq!(
        format_of(&stale_text),
        1,
        "a memo was not verified: the stamps are kept as they were"
    );
    let again = restore_text(&stale_text);
    let executed = EXECUTED.load(Ordering::SeqCst);
    let value = from_left(&again, keys(&again)[3]).clone();
    assert_eq!(value, "3:2", "the memo is out of date, and runs again");
    assert!(
        EXECUTED.load(Ordering::SeqCst) > executed,
        "it ran again, and was not taken as it was saved"
    );
}

/// A state saved with canonical stamps is as sensitive to a change as one saved with its stamps:
/// what read the input directly, through an interned key and through a tracked struct's field is
/// found out of date by the run that restored it, and runs again.
#[test]
fn a_change_after_a_canonical_restore_finds_what_depends_on_it_out_of_date() {
    let mut db = Db::default();
    Left::new(&db, 1);
    Right::new(&db, 1);
    populate(&db);
    let expected_before = sized(&db);
    let (text, _) = saved(&mut db);
    assert_eq!(
        format_of(&text),
        2,
        "the state is saved with canonical stamps"
    );

    let mut restored = restore_text(&text);
    Left::get(&restored).set_value(&mut restored).to(2);
    let executed = EXECUTED.load(Ordering::SeqCst);
    assert_eq!(
        from_left(&restored, keys(&restored)[3]).as_str(),
        "3:2",
        "read through the input directly and an interned key"
    );
    assert!(
        EXECUTED.load(Ordering::SeqCst) > executed,
        "the memo was found out of date and ran again"
    );
    assert_eq!(
        from_right(&restored, keys(&restored)[3]).as_str(),
        "3:1",
        "what did not read the left input is as it was"
    );
    // Through a tracked struct's field: the parts are made again from the new left input.
    assert_eq!(
        sized(&restored),
        expected_before + 4,
        "read through a field of a tracked struct that the query made again"
    );
}
