//! The ledger says what each query holds by dropping it and seeing what the process gives back.
//!
//! The toy database has one query whose few values are large and one whose many values are tiny. The
//! ledger must credit the memory to the large one, give a floor below what was held before, and leave
//! the queries able to compute what was dropped.

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

static BIG_RUNS: AtomicUsize = AtomicUsize::new(0);

/// How big each large value is, and how many there are.
const BIG_BYTES: usize = 8 << 20;
const BIGS: usize = 6;
const SMALLS: usize = 300;

/// A query with a few large values (touched, so that they are resident).
#[salsa::tracked(returns(ref))]
fn big<'db>(db: &'db dyn Database, key: Key<'db>) -> Vec<u8> {
    BIG_RUNS.fetch_add(1, Ordering::SeqCst);
    vec![u8::try_from(key.n(db) % 200).unwrap_or(1) + 1; BIG_BYTES + Source::get(db).value(db)]
}

/// A query with many tiny values.
#[salsa::tracked(returns(ref))]
fn small<'db>(db: &'db dyn Database, key: Key<'db>) -> Vec<u8> {
    vec![7; 64 + key.n(db) % 8]
}

fn read(db: &Db) {
    for n in 0..BIGS {
        let _ = big(db, Key::new(db, n));
    }
    for n in 0..SMALLS {
        let _ = small(db, Key::new(db, 1_000 + n));
    }
}

/// What a query gave back, from the ledger's rows as `(query, memos, freed KiB)`.
fn freed(rows: &[(String, u64, i64)], query: &str) -> Option<i64> {
    rows.iter()
        .find(|(name, _, _)| name == query)
        .map(|(_, _, freed)| *freed)
}

#[test]
fn the_ledger_credits_the_memory_to_the_query_that_held_it() {
    let mut db = Db::default();
    Source::new(&db, 0);
    read(&db);

    let ledger = cordial::itemize(&mut db, &[]).unwrap_or_else(|| panic!("no memory reading"));
    let rows: Vec<(String, u64, i64)> = ledger
        .rows()
        .iter()
        .map(|row| (row.query().to_owned(), row.memos(), row.freed_kib()))
        .collect();

    let held = u64::try_from(BIGS * BIG_BYTES / 1024).unwrap_or(0);
    let big_freed = freed(&rows, "big").unwrap_or_else(|| panic!("no row for big: {rows:?}"));
    assert!(
        big_freed >= i64::try_from(held * 8 / 10).unwrap_or(0),
        "the large query gave back most of what it held ({held} KiB): {rows:?}"
    );
    let small_freed = freed(&rows, "small").unwrap_or(0);
    assert!(
        small_freed < i64::try_from(held / 10).unwrap_or(0),
        "and the tiny one gave back a small part of that: {rows:?}"
    );
    assert_eq!(
        rows.iter()
            .find(|(name, _, _)| name == "big")
            .map(|(_, memos, _)| *memos),
        Some(BIGS as u64),
        "every memo was counted"
    );
    assert!(
        ledger.before_kib() - ledger.floor_kib() >= held * 8 / 10,
        "the floor is below what was held by about what the caches held: {rows:?}"
    );

    // Dropped is not lost: the large query computes its values again.
    let before = BIG_RUNS.load(Ordering::SeqCst);
    read(&db);
    assert_eq!(BIG_RUNS.load(Ordering::SeqCst) - before, BIGS);
}
