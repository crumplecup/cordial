//! The ledger says what each query holds by dropping it and seeing what the process gives back.
//!
//! The toy database has one query whose few values are large and one whose many values are tiny. The
//! ledger must credit the memory to the large one, give a floor below what was held before, and leave
//! the queries able to compute what was dropped.

use std::sync::atomic::{AtomicUsize, Ordering};

use cordial::{
    CrateTarget, MetricsLayer, SemanticCargoOptions, SourceDatabase, StoreLayout, read_run_metrics,
    write_run_metrics,
};
use miette::{IntoDiagnostic, Result};
use salsa::{Database, Storage};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

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

/// The names of the threads that dropped a [`Tagged`].
static DROPPED_ON: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// A value that says which thread dropped it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::SalsaValue)]
struct Tagged(usize);

impl Drop for Tagged {
    fn drop(&mut self) {
        if let Ok(mut dropped) = DROPPED_ON.lock() {
            dropped.push(std::thread::current().name().unwrap_or("").to_owned());
        }
    }
}

/// A query whose values say which thread dropped them.
#[salsa::tracked(returns(ref))]
fn tagged<'db>(db: &'db dyn Database, key: Key<'db>) -> Tagged {
    Tagged(*key.n(db))
}

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

/// A run that asks for it says, at the end of the evidence stage, what each query held and what the
/// process held besides: the ledger's rows and its totals.
#[test]
fn a_run_that_asks_for_it_says_what_each_query_held() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='ledgered'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library: String = (0..30)
        .map(|index| format!("pub fn f{index}() -> usize {{ {index} }}\n"))
        .collect();
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let options = SemanticCargoOptions::default().with_memory_ledger(true);
    let mut session = SourceDatabase::load_saved(root.path(), &options, None).into_diagnostic()?;
    session
        .project(
            &CrateTarget::new("ledgered", root.path()),
            &[root.path().join("src/lib.rs")],
        )
        .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(store.path().to_path_buf(), "ledgered".to_owned());
    write_run_metrics(&layout).into_diagnostic()?;
    let metrics = read_run_metrics(&layout);
    let values = metrics.values();
    let rows = values
        .iter()
        .filter(|(key, _)| key.starts_with("ledger.") && key.ends_with(".memos"))
        .count();
    assert!(rows > 5, "a row for each query that held values: {rows}");
    let total = |name: &str| values.iter().find(|(key, _)| *key == name).map(|(_, v)| *v);
    assert!(
        total("ledger.evidence.file_texts").is_some_and(|texts| texts > 0),
        "the files' texts"
    );
    assert!(
        total("ledger.evidence.threads").is_some_and(|threads| threads > 0),
        "the threads"
    );
    // Whether the floor is below the start is for the toy database above, whose values are large
    // enough to tell from the process's own noise; here both are said.
    assert!(
        total("ledger.evidence.before_kib").is_some(),
        "what was held before"
    );
    assert!(
        total("ledger.evidence.floor_kib").is_some(),
        "and what was left"
    );
    Ok(())
}

/// Each query is dropped on a thread of its own, named in its row: what a profile that counts the
/// bytes each thread frees says of that thread is what that query's values were.
#[test]
fn each_query_is_dropped_on_the_thread_its_row_names() {
    let mut db = Db::default();
    Source::new(&db, 0);
    read(&db);
    for n in 0..5 {
        let _ = tagged(&db, Key::new(&db, 2_000 + n));
    }

    let ledger = cordial::itemize(&mut db, &[]).unwrap_or_else(|| panic!("no memory reading"));
    let threads: Vec<(String, String)> = ledger
        .rows()
        .iter()
        .map(|row| (row.query().to_owned(), row.thread().to_owned()))
        .collect();
    let named: std::collections::BTreeSet<&String> =
        threads.iter().map(|(_, thread)| thread).collect();
    assert_eq!(
        named.len(),
        threads.len(),
        "a thread for each query: {threads:?}"
    );

    let tagged_thread = threads
        .iter()
        .find(|(query, _)| query == "tagged")
        .map(|(_, thread)| thread.clone())
        .unwrap_or_else(|| panic!("no row for tagged: {threads:?}"));
    let dropped = DROPPED_ON
        .lock()
        .map(|dropped| dropped.clone())
        .unwrap_or_default();
    assert_eq!(dropped.len(), 5, "every value was dropped");
    assert!(
        dropped.iter().all(|name| *name == tagged_thread),
        "on the thread the query's row names ({tagged_thread}): {dropped:?}"
    );
}
