//! Whether dropping the values of a query that evicts the least recently used gives their memory
//! back, with only Salsa and an allocator that counts: the smallest program that has the
//! phenomenon, so that what is added to it (a query that returns a clone, a thread, a query that
//! reads the value) can be told from what is not.
//!
//! A query holds a large value for each of many inputs and keeps the `CAPACITY` most recently
//! used; evicting must give back the memory of the rest.

#[path = "../src/counting_allocator.rs"]
mod counting_allocator;

use salsa::{Database, Setter, Storage};
use std::sync::Arc;

#[global_allocator]
static ALLOCATOR: counting_allocator::Counting = counting_allocator::Counting;

/// How many values the query keeps.
const CAPACITY: usize = 16;
/// How many inputs there are, so how many values are made.
const INPUTS: usize = 200;
/// The size of a value.
const VALUE_BYTES: usize = 256 * 1024;

#[salsa::input]
struct Doc {
    #[returns(copy)]
    seed: u8,
}

#[salsa::db]
#[derive(Clone, Default)]
struct Db {
    storage: Storage<Self>,
}

#[salsa::db]
impl Database for Db {}

/// Returned by reference, as the parse of a macro expansion is.
#[salsa::tracked(lru = 16, returns(ref))]
fn by_reference(db: &dyn Database, doc: Doc) -> Vec<u8> {
    vec![doc.seed(db); VALUE_BYTES]
}

/// Returned as a clone of a shared value, as the parse of a file is.
#[salsa::tracked(lru = 16, returns(clone))]
fn by_clone(db: &dyn Database, doc: Doc) -> Arc<Vec<u8>> {
    Arc::new(vec![doc.seed(db); VALUE_BYTES])
}

/// Reads the value of another query and keeps something small of it.
#[salsa::tracked(returns(copy))]
fn reads_by_clone(db: &dyn Database, doc: Doc) -> usize {
    by_clone(db, doc).len()
}

/// A key that is an interned struct, as the file and the macro call of the parses are.
#[salsa::interned]
struct Key<'db> {
    n: u32,
    seed: u8,
}

/// Keyed by an interned struct, returned as a clone.
#[salsa::tracked(lru = 16, returns(clone))]
fn by_interned<'db>(db: &'db dyn Database, key: Key<'db>) -> Arc<Vec<u8>> {
    Arc::new(vec![*key.seed(db); VALUE_BYTES])
}

/// Keyed by an interned struct, returned by reference.
#[salsa::tracked(lru = 16, returns(ref))]
fn by_interned_reference<'db>(db: &'db dyn Database, key: Key<'db>) -> Vec<u8> {
    vec![*key.seed(db); VALUE_BYTES]
}

/// A database, with the program's allocator told to Salsa as in a run.
fn fresh() -> Db {
    salsa::set_allocation_probe(counting_allocator::net_bytes);
    salsa::set_allocation_tags(salsa::AllocationTags {
        set_tag: counting_allocator::set_tag,
        held_by_tag: counting_allocator::held_by_tag,
        held_in_all: counting_allocator::held_in_all,
        peak_held: counting_allocator::peak_held,
        freed_pairs: counting_allocator::freed_pairs,
        freed_by_age: counting_allocator::freed_by_age,
    });
    Db::default()
}

fn held() -> i64 {
    counting_allocator::held_in_all()
}

fn docs(db: &Db) -> Vec<Doc> {
    (0..INPUTS)
        .map(|n| Doc::new(db, u8::try_from(n % 250).unwrap_or(0)))
        .collect()
}

/// What evicting gives back, in bytes, and how much it should: the values past the capacity.
///
/// Before it, the memos hold every value that was made: the capacity of a query is not a bound
/// while queries run, since the values past it are dropped only when Salsa is asked to (or a
/// revision starts), with the database held exclusively. That is what a run that never asks keeps.
fn eviction_gives_back(db: &mut Db, before: i64) -> (i64, i64) {
    let made = held() - before;
    let all = i64::try_from(INPUTS * VALUE_BYTES).unwrap_or(0);
    assert!(
        made >= all * 9 / 10,
        "the memos hold {} MB of the {} MB made, though the capacity is {CAPACITY} of {INPUTS}",
        made / 1_000_000,
        all / 1_000_000
    );
    db.trigger_lru_eviction();
    let back = made - (held() - before);
    let should = i64::try_from((INPUTS - CAPACITY) * VALUE_BYTES).unwrap_or(0);
    (back, should)
}

fn assert_gives_back(back: i64, should: i64, what: &str) {
    assert!(
        back >= should * 9 / 10,
        "{what}: evicting gave back {} MB of the {} MB that were past the capacity",
        back / 1_000_000,
        should / 1_000_000
    );
}

#[test]
fn a_query_returned_by_reference_gives_its_values_back() {
    let mut db = fresh();
    let docs = docs(&db);
    let before = held();
    for doc in &docs {
        assert_eq!(by_reference(&db, *doc).len(), VALUE_BYTES);
    }
    let (back, should) = eviction_gives_back(&mut db, before);
    assert_gives_back(back, should, "returned by reference");
}

#[test]
fn a_query_returned_as_a_clone_gives_its_values_back() {
    let mut db = fresh();
    let docs = docs(&db);
    let before = held();
    for doc in &docs {
        assert_eq!(by_clone(&db, *doc).len(), VALUE_BYTES);
    }
    let (back, should) = eviction_gives_back(&mut db, before);
    assert_gives_back(back, should, "returned as a clone");
}

#[test]
fn values_made_on_other_threads_are_given_back() {
    let mut db = fresh();
    let docs = docs(&db);
    let before = held();
    std::thread::scope(|scope| {
        for part in docs.chunks(INPUTS / 4) {
            let handle = db.clone();
            scope.spawn(move || {
                for doc in part {
                    assert_eq!(by_clone(&handle, *doc).len(), VALUE_BYTES);
                }
            });
        }
    });
    let (back, should) = eviction_gives_back(&mut db, before);
    assert_gives_back(back, should, "made on other threads");
}

#[test]
fn values_a_query_read_are_given_back() {
    let mut db = fresh();
    let docs = docs(&db);
    let before = held();
    for doc in &docs {
        assert_eq!(reads_by_clone(&db, *doc), VALUE_BYTES);
    }
    let (back, should) = eviction_gives_back(&mut db, before);
    assert_gives_back(back, should, "read by another query");
}

#[test]
fn a_revision_gives_the_values_back_too() {
    let mut db = fresh();
    let docs = docs(&db);
    let before = held();
    for doc in &docs {
        assert_eq!(by_clone(&db, *doc).len(), VALUE_BYTES);
    }
    let made = held() - before;
    // An input changed: a new revision, which resets what is over capacity as eviction does.
    docs[0].set_seed(&mut db).to(9);
    let back = made - (held() - before);
    let should = i64::try_from((INPUTS - CAPACITY) * VALUE_BYTES).unwrap_or(0);
    assert_gives_back(back, should, "a new revision");
}

#[test]
fn a_query_keyed_by_an_interned_struct_returned_as_a_clone_gives_its_values_back() {
    let mut db = fresh();
    let before = held();
    for n in 0..INPUTS {
        let key = Key::new(&db, u32::try_from(n).unwrap_or(0), 7);
        assert_eq!(by_interned(&db, key).len(), VALUE_BYTES);
    }
    let (back, should) = eviction_gives_back(&mut db, before);
    assert_gives_back(back, should, "keyed by an interned struct, as a clone");
}

#[test]
fn a_query_keyed_by_an_interned_struct_returned_by_reference_gives_its_values_back() {
    let mut db = fresh();
    let before = held();
    for n in 0..INPUTS {
        let key = Key::new(&db, u32::try_from(n).unwrap_or(0), 7);
        assert_eq!(by_interned_reference(&db, key).len(), VALUE_BYTES);
    }
    let (back, should) = eviction_gives_back(&mut db, before);
    assert_gives_back(back, should, "keyed by an interned struct, by reference");
}
