//! Flattening the dependencies of persisted queries is done once per query, not once per
//! persisted query that reaches it, and what is saved still covers every dependency.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use salsa::{Database, Setter, Storage};
use tracing_subscriber::layer::SubscriberExt as _;

#[salsa::input(persist, singleton)]
struct Source {
    #[returns(copy)]
    value: usize,
}

/// The argument of every query: a number, interned.
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

/// A chain of queries that are not persisted, each reading the one below and the input.
#[salsa::tracked(returns(copy))]
fn layer<'db>(db: &'db dyn Database, depth: Key<'db>) -> usize {
    let source = Source::get(db).value(db);
    match depth.n(db) {
        0 => source,
        n => layer(db, Key::new(db, n - 1)) + source,
    }
}

/// Two queries that call each other and settle: a cycle between queries not persisted.
#[salsa::tracked(returns(copy), cycle_initial = ping_initial)]
fn ping<'db>(db: &'db dyn Database, key: Key<'db>) -> usize {
    pong(db, key).max(Source::get(db).value(db))
}

fn ping_initial<'db>(_db: &'db dyn Database, _id: salsa::Id, _key: Key<'db>) -> usize {
    0
}

#[salsa::tracked(returns(copy), cycle_initial = pong_initial)]
fn pong<'db>(db: &'db dyn Database, key: Key<'db>) -> usize {
    ping(db, key).min(Source::get(db).value(db))
}

fn pong_initial<'db>(_db: &'db dyn Database, _id: salsa::Id, _key: Key<'db>) -> usize {
    0
}

/// Queries that all call each other, as rust-analyzer's do: walked along every path through
/// them, a group of this many is walked this many factorial times.
const CLUSTER: usize = 14;

#[salsa::tracked(returns(copy), cycle_initial = cluster_initial)]
fn cluster<'db>(db: &'db dyn Database, key: Key<'db>) -> usize {
    let me = *key.n(db);
    let others = (0..CLUSTER)
        .filter(|other| *other != me)
        .map(|other| cluster(db, Key::new(db, other)))
        .max()
        .unwrap_or(0);
    others.min(Source::get(db).value(db))
}

fn cluster_initial<'db>(_db: &'db dyn Database, _id: salsa::Id, _key: Key<'db>) -> usize {
    0
}

static EXECUTED: AtomicUsize = AtomicUsize::new(0);

/// Persisted, and reads the chain and the cycle.
#[salsa::tracked(returns(copy), persist)]
fn persisted<'db>(db: &'db dyn Database, k: Key<'db>) -> usize {
    EXECUTED.fetch_add(1, Ordering::SeqCst);
    layer(db, Key::new(db, DEPTH))
        + ping(db, Key::new(db, 0))
        + cluster(db, Key::new(db, 0))
        + k.n(db)
}

const DEPTH: usize = 60;
const PERSISTED: usize = 40;

/// What the persisted queries' flattening said, from the events it logs.
#[derive(Default, Clone)]
struct Reports(Arc<Mutex<Vec<Report>>>);

/// One ingredient's report: its name, memos, queries walked and queries reused.
type Report = (String, u64, u64, u64, u64);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Reports {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        if event.metadata().target() != "salsa::persistence" {
            return;
        }
        #[derive(Default)]
        struct Fields {
            ingredient: String,
            memos: u64,
            walked: u64,
            reused: u64,
            sets: u64,
        }
        impl tracing::field::Visit for Fields {
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "memos" => self.memos = value,
                    "walked" => self.walked = value,
                    "reused" => self.reused = value,
                    "sets" => self.sets = value,
                    _ => {}
                }
            }
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "ingredient" {
                    self.ingredient = value.to_owned();
                }
            }
            fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        if let Ok(mut reports) = self.0.lock() {
            reports.push((
                fields.ingredient,
                fields.memos,
                fields.walked,
                fields.reused,
                fields.sets,
            ));
        }
    }
}

#[test]
fn a_shared_graph_is_walked_once_and_what_is_saved_still_covers_it() {
    let reports = Reports::default();
    let subscriber = tracing_subscriber::registry().with(reports.clone());
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut db = Db::default();
    Source::new(&db, 1);
    let expected: Vec<usize> = (0..PERSISTED)
        .map(|k| persisted(&db, Key::new(&db, k)))
        .collect();
    assert_eq!(EXECUTED.load(Ordering::SeqCst), PERSISTED);

    let saved = serde_json::to_string(&<dyn Database>::as_serialize(&mut db))
        .unwrap_or_else(|error| panic!("serializing: {error}"));

    // Every persisted memo reads the same chain and cycle; they are walked once between
    // them, not once each (that would be PERSISTED times as many).
    let (memos, walked, reused, sets) = reports
        .0
        .lock()
        .map(|reports| {
            reports
                .iter()
                .filter(|(name, ..)| name.contains("persisted"))
                .map(|(_, memos, walked, reused, sets)| (*memos, *walked, *reused, *sets))
                .fold((0, 0, 0, 0), |(a, b, c, d), (e, f, g, h)| {
                    (a + e, b + f, c + g, d + h)
                })
        })
        .unwrap_or_default();
    assert_eq!(memos, PERSISTED as u64);
    let nodes = (DEPTH + 1 + 2 + CLUSTER) as u64; // the chain, ping and pong, the cluster
    assert!(
        walked <= nodes + 4,
        "walked {walked} queries for {memos} memos over a graph of {nodes}"
    );

    // They all depend on the same graph, so the file holds its edges once, not once each.
    assert!(
        sets <= 6,
        "the {memos} memos refer to {sets} distinct sets of edges, not one each"
    );
    assert!(
        reused >= PERSISTED as u64 - 1,
        "the memos after the first were answered from what the first found: {reused}"
    );

    // Restored: nothing is computed again, because nothing changed...
    let before = EXECUTED.load(Ordering::SeqCst);
    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&saved),
    )
    .unwrap_or_else(|error| panic!("deserializing: {error}"));
    let again: Vec<usize> = (0..PERSISTED)
        .map(|k| persisted(&restored, Key::new(&restored, k)))
        .collect();
    assert_eq!(again, expected);
    assert_eq!(
        EXECUTED.load(Ordering::SeqCst),
        before,
        "restored memos were reused"
    );

    // ...and when the input changes, every one that reached it through the chain and the
    // cycle is computed again: the flattened edges cover what the queries between read.
    Source::get(&restored).set_value(&mut restored).to(2);
    let changed: Vec<usize> = (0..PERSISTED)
        .map(|k| persisted(&restored, Key::new(&restored, k)))
        .collect();
    assert_ne!(changed, expected);
    assert_eq!(
        EXECUTED.load(Ordering::SeqCst),
        before + PERSISTED,
        "each memo noticed the change"
    );
}

#[salsa::input(persist, singleton)]
struct Library {
    #[returns(copy)]
    value: usize,
}

/// Reads the input that changes (workspace) and the one that rarely does (library).
#[salsa::tracked(returns(copy), persist)]
fn reads_both<'db>(db: &'db dyn Database, key: Key<'db>) -> usize {
    EXECUTED_BOTH.fetch_add(1, Ordering::SeqCst);
    Source::get(db).value(db) + Library::get(db).value(db) + key.n(db)
}

/// Reads only the workspace's input.
#[salsa::tracked(returns(copy), persist)]
fn reads_workspace<'db>(db: &'db dyn Database, key: Key<'db>) -> usize {
    EXECUTED_WORKSPACE.fetch_add(1, Ordering::SeqCst);
    Source::get(db).value(db) + key.n(db)
}

static EXECUTED_BOTH: AtomicUsize = AtomicUsize::new(0);
static EXECUTED_WORKSPACE: AtomicUsize = AtomicUsize::new(0);

#[test]
fn what_rarely_changes_is_one_edge_and_still_invalidates_what_read_it() {
    let mut db = Db::default();
    Source::new(&db, 1);
    let _library = Library::builder(10)
        .durability(salsa::Durability::HIGH)
        .new(&db);
    let key = Key::new(&db, 0);
    assert_eq!(reads_both(&db, key), 11);
    assert_eq!(reads_workspace(&db, key), 1);

    let saved = serde_json::to_string(&<dyn Database>::as_serialize(&mut db))
        .unwrap_or_else(|error| panic!("serializing: {error}"));
    // The library input is not an edge of its own: one edge stands for it, so a memo that
    // reads it carries two edges (the workspace input and the watermark), not thousands.
    assert!(
        !saved.contains("Library"),
        "no edge names the library input: {saved}"
    );

    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&saved),
    )
    .unwrap_or_else(|error| panic!("deserializing: {error}"));
    let key = Key::new(&restored, 0);
    let (both, workspace) = (
        EXECUTED_BOTH.load(Ordering::SeqCst),
        EXECUTED_WORKSPACE.load(Ordering::SeqCst),
    );

    // Nothing changed: both are reused.
    assert_eq!(reads_both(&restored, key), 11);
    assert_eq!(reads_workspace(&restored, key), 1);
    assert_eq!(EXECUTED_BOTH.load(Ordering::SeqCst), both);
    assert_eq!(EXECUTED_WORKSPACE.load(Ordering::SeqCst), workspace);

    // The workspace's input changes: only what read it is computed again.
    Source::get(&restored).set_value(&mut restored).to(2);
    let key = Key::new(&restored, 0);
    assert_eq!(reads_both(&restored, key), 12);
    assert_eq!(reads_workspace(&restored, key), 2);
    assert_eq!(EXECUTED_BOTH.load(Ordering::SeqCst), both + 1);
    assert_eq!(EXECUTED_WORKSPACE.load(Ordering::SeqCst), workspace + 1);

    // The library's input changes: what read it is computed again, whichever of its inputs.
    Library::get(&restored).set_value(&mut restored).to(20);
    let key = Key::new(&restored, 0);
    assert_eq!(reads_both(&restored, key), 22);
    assert_eq!(EXECUTED_BOTH.load(Ordering::SeqCst), both + 2);
}

/// An interned struct that is not persisted.
#[salsa::interned]
struct Plain<'db> {
    n: usize,
}

#[salsa::input(persist, singleton)]
struct Unrelated {
    #[returns(copy)]
    value: usize,
}

/// Not persisted, keyed by an interned value.
#[salsa::tracked(returns(copy))]
fn keyed_by_plain<'db>(db: &'db dyn Database, key: Plain<'db>) -> usize {
    Source::get(db).value(db) + key.n(db)
}

/// Whether the query that interns the key has read an input yet.
///
/// An interned value takes the durability of what its creator read before interning it. One
/// interned first has the highest durability and is never reused, so the database never treats
/// it as stale. One interned after a read of an input that changes is reusable, and is stale
/// once revisions pass without it being interned again: the case rust-analyzer's keys are in.
#[derive(Clone, Copy, Debug)]
enum Scenario {
    SomethingToCatch,
    NothingToSeeHere,
}

#[salsa::input(persist, singleton)]
struct Mode {
    #[returns(copy)]
    read_before_interning: bool,
}

#[salsa::tracked(returns(copy), persist)]
fn reads_keyed_by_plain(db: &dyn Database) -> usize {
    if Mode::get(db).read_before_interning(db) {
        let _ = Unrelated::get(db).value(db);
    }
    keyed_by_plain(db, Plain::new(db, 7))
}

/// Compute the query, let revisions pass that neither read it nor intern its key again, save,
/// restore, and ask again.
fn save_and_restore(scenario: Scenario) -> usize {
    let mut db = Db::default();
    Source::new(&db, 1);
    Mode::new(&db, matches!(scenario, Scenario::SomethingToCatch));
    let unrelated = Unrelated::new(&db, 0);
    assert_eq!(reads_keyed_by_plain(&db), 8);

    for step in 1..=4 {
        unrelated.set_value(&mut db).to(step);
    }

    // Saving looks at the memo of the query behind the interned key, which was last interned
    // revisions ago.
    let saved = serde_json::to_string(&<dyn Database>::as_serialize(&mut db))
        .unwrap_or_else(|error| panic!("serializing {scenario:?}: {error}"));

    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&saved),
    )
    .unwrap_or_else(|error| panic!("deserializing {scenario:?}: {error}"));
    reads_keyed_by_plain(&restored)
}

/// Saving must not look at a stale interned key through the checked accessor: it refused
/// (a panic, "was not interned in the latest revision for its durability") and a cold run of
/// cordial died in its save. `SomethingToCatch` is that case; `NothingToSeeHere` never could be.
#[test]
fn a_memo_behind_a_stale_interned_key_can_be_saved_whether_or_not_there_is_one() {
    for scenario in [Scenario::SomethingToCatch, Scenario::NothingToSeeHere] {
        assert_eq!(save_and_restore(scenario), 8, "{scenario:?}");
    }
}
