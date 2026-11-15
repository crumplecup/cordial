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
type Report = (String, u64, u64, u64);

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
        }
        impl tracing::field::Visit for Fields {
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                match field.name() {
                    "memos" => self.memos = value,
                    "walked" => self.walked = value,
                    "reused" => self.reused = value,
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
    let (memos, walked, reused) = reports
        .0
        .lock()
        .map(|reports| {
            reports
                .iter()
                .filter(|(name, ..)| name.contains("persisted"))
                .map(|(_, memos, walked, reused)| (*memos, *walked, *reused))
                .fold((0, 0, 0), |(a, b, c), (d, e, f)| (a + d, b + e, c + f))
        })
        .unwrap_or_default();
    assert_eq!(memos, PERSISTED as u64);
    let nodes = (DEPTH + 1 + 2 + CLUSTER) as u64; // the chain, ping and pong, the cluster
    assert!(
        walked <= nodes + 4,
        "walked {walked} queries for {memos} memos over a graph of {nodes}"
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
