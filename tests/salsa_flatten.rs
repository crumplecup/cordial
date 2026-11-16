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

#[salsa::input(persist, singleton)]
struct Switch {
    #[returns(copy)]
    value: usize,
}

#[salsa::input(persist, singleton)]
struct SideA {
    #[returns(copy)]
    value: usize,
}

#[salsa::input(persist, singleton)]
struct SideB {
    #[returns(copy)]
    value: usize,
}

/// Not persisted; which input it reads depends on the switch, and its answer is the same either way.
#[salsa::tracked(returns(copy))]
fn middle(db: &dyn Database) -> usize {
    if Switch::get(db).value(db) == 0 {
        SideA::get(db).value(db)
    } else {
        SideB::get(db).value(db)
    }
}

static EXECUTED_OUTER: AtomicUsize = AtomicUsize::new(0);

#[salsa::tracked(returns(copy), persist)]
fn outer(db: &dyn Database) -> usize {
    EXECUTED_OUTER.fetch_add(1, Ordering::SeqCst);
    middle(db) + 1
}

/// What is done between the rerun of `middle` and the save.
#[derive(Clone, Copy, Debug)]
enum BeforeSaving {
    /// Nothing: `outer` was last verified before `middle` ran again.
    SomethingToCatch,
    /// `outer` is asked for, which verifies it.
    AskedOuter,
    /// Every persisted memo is verified, as cordial does before it saves.
    VerifiedPersisted,
}

/// Whether `outer` ran again in a database restored from one saved after the switch was flipped
/// and `middle` (not `outer`) was asked, and then `before` was done.
fn outer_runs_again(before: BeforeSaving) -> bool {
    let mut db = Db::default();
    Switch::new(&db, 0);
    SideA::new(&db, 1);
    SideB::new(&db, 1);
    assert_eq!(outer(&db), 2);

    // The switch flips; `middle` is asked on its own and now depends on the other side. Its
    // answer is the same, so `outer` is as valid as it was, but nothing has verified it since.
    Switch::get(&db).set_value(&mut db).to(1);
    assert_eq!(middle(&db), 1);
    match before {
        BeforeSaving::SomethingToCatch => {}
        BeforeSaving::AskedOuter => assert_eq!(outer(&db), 2),
        BeforeSaving::VerifiedPersisted => <dyn Database>::verify_persisted_memos(&db),
    }

    let saved = serde_json::to_string(&<dyn Database>::as_serialize(&mut db))
        .unwrap_or_else(|error| panic!("serializing {before:?}: {error}"));
    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&saved),
    )
    .unwrap_or_else(|error| panic!("deserializing {before:?}: {error}"));

    let executed = EXECUTED_OUTER.load(Ordering::SeqCst);
    assert_eq!(outer(&restored), 2, "{before:?}");
    EXECUTED_OUTER.load(Ordering::SeqCst) != executed
}

/// A persisted query is saved with what its dependencies are now, and with the revision it was
/// last verified at. If a query between them has run again since, onto inputs that did not exist
/// when it was verified, a restored database finds the persisted query out of date though nothing
/// it depends on changed, unless it was verified before the save.
#[test]
fn a_persisted_query_is_valid_when_restored_if_it_was_verified_before_the_save() {
    assert!(
        outer_runs_again(BeforeSaving::SomethingToCatch),
        "without verifying, a restored `outer` is out of date: this is what the others prevent"
    );
    assert!(
        !outer_runs_again(BeforeSaving::AskedOuter),
        "asked before saving, it was verified at the revision it is saved with"
    );
    assert!(
        !outer_runs_again(BeforeSaving::VerifiedPersisted),
        "every persisted memo verified before the save is valid when restored"
    );
}

#[salsa::input(persist)]
struct Page {
    #[returns(copy)]
    value: usize,
}

#[salsa::input(persist, singleton)]
struct Pages {
    #[returns(ref)]
    all: Vec<Page>,
}

static EXECUTED_READERS: AtomicUsize = AtomicUsize::new(0);

/// Persisted, and reads every page, as a function of a crate reads the files of every crate.
#[salsa::tracked(returns(copy), persist)]
fn reader<'db>(db: &'db dyn Database, k: Key<'db>) -> usize {
    EXECUTED_READERS.fetch_add(1, Ordering::SeqCst);
    Pages::get(db)
        .all(db)
        .iter()
        .map(|page| page.value(db))
        .sum::<usize>()
        + k.n(db)
}

/// How long a restored database takes to find `readers` memos that each read `pages` inputs
/// still valid, and how many ran again.
fn verify_restored(pages: usize, readers: usize) -> (std::time::Duration, usize) {
    let mut db = Db::default();
    let all = (0..pages).map(|n| Page::new(&db, n)).collect();
    Pages::new(&db, all);
    Unrelated::new(&db, 0);
    for n in 0..readers {
        reader(&db, Key::new(&db, n));
    }
    let saved = serde_json::to_string(&<dyn Database>::as_serialize(&mut db))
        .unwrap_or_else(|error| panic!("serializing: {error}"));
    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&saved),
    )
    .unwrap_or_else(|error| panic!("deserializing: {error}"));
    Unrelated::get(&restored).set_value(&mut restored).to(1);
    let executed = EXECUTED_READERS.load(Ordering::SeqCst);
    let started = std::time::Instant::now();
    for n in 0..readers {
        reader(&restored, Key::new(&restored, n));
    }
    (
        started.elapsed(),
        EXECUTED_READERS.load(Ordering::SeqCst) - executed,
    )
}

#[test]
fn restored_memos_over_many_inputs_are_verified_quickly() {
    let (elapsed, executed) = verify_restored(2000, 1660);
    assert_eq!(executed, 0, "nothing changed");
    // Measured at 20 ms in release; the bound only catches verification turning into a walk of
    // every input for every memo again.
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "verifying {executed} restored memos over many inputs took {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Memos that share what their dependencies flatten to keep sharing it once restored.
// ---------------------------------------------------------------------------------------------

/// One input a group of queries reads.
#[salsa::input(persist)]
struct Cell {
    #[returns(copy)]
    value: usize,
}

/// The inputs a group of queries all read, through a query that is not persisted.
#[salsa::input(persist)]
struct Group {
    #[returns(ref)]
    cells: Vec<Cell>,
}

/// Not persisted: everything a persisted query of the group reaches goes through here, so the
/// group's queries all flatten to the same set of edges, one for each cell and the library.
#[salsa::tracked(returns(copy))]
fn total(db: &dyn Database, group: Group) -> usize {
    let cells: usize = group.cells(db).iter().map(|cell| cell.value(db)).sum();
    cells + Library::get(db).value(db)
}

static MEMBER_EXECUTIONS: [AtomicUsize; 2] = [AtomicUsize::new(0), AtomicUsize::new(0)];

/// Persisted, and reads its group's cells (through `total`) and its own number.
#[salsa::tracked(returns(copy), persist)]
fn member<'db>(db: &'db dyn Database, group: Group, key: Key<'db>) -> usize {
    MEMBER_EXECUTIONS[*key.n(db) % 2].fetch_add(1, Ordering::SeqCst);
    total(db, group) + key.n(db)
}

/// Cells each group reads, queries in each group, and how many of the cells both groups read.
const CELLS: usize = 200;
const MEMBERS: usize = 30;
const COMMON: usize = 40;

/// Two groups of cells and the key of each member: the members with an even key are of the first
/// group, the odd of the second. The first `COMMON` cells of each group are the same cells, then
/// each group has cells of its own: the edges of the first group's set are numbered one after another,
/// and the second's are the common ones and, after the first group's own, its own: a gap.
struct Fixture {
    groups: [Group; 2],
    cells: [Vec<Cell>; 2],
}

impl Fixture {
    fn new(db: &Db) -> Self {
        let _library = Library::builder(10)
            .durability(salsa::Durability::HIGH)
            .new(db);
        let common: Vec<Cell> = (0..COMMON).map(|n| Cell::new(db, n)).collect();
        let cells = [0, 1].map(|group| {
            common
                .iter()
                .copied()
                .chain((COMMON..CELLS).map(|n| Cell::new(db, group * 1000 + n)))
                .collect::<Vec<_>>()
        });
        let groups = [0, 1].map(|group| Group::new(db, cells[group].clone()));
        Self { groups, cells }
    }

    /// Ask for every member of both groups, summing what each group answers.
    fn ask_all(&self, db: &Db) -> [usize; 2] {
        let mut answers = [0, 0];
        for n in 0..MEMBERS * 2 {
            answers[n % 2] += member(db, self.groups[n % 2], Key::new(db, n));
        }
        answers
    }
}

/// What a restored database is asked to change before its queries are asked for again.
#[derive(Clone, Copy, Debug)]
enum Change {
    /// A cell of the first group: every query of that group reaches it through the set.
    InTheFirstGroup,
    /// A cell of the second group.
    InTheSecondGroup,
    /// A cell both groups read: it is in both sets, as the start of one run in each.
    InTheCommonCells,
    /// The library's input, which every query reaches through the watermark in its set.
    InTheLibrary,
    /// A cell no query reads.
    NothingToSee,
}

impl Change {
    /// The queries of each group this change makes compute again.
    fn reaches(self) -> [usize; 2] {
        match self {
            Self::InTheFirstGroup => [MEMBERS, 0],
            Self::InTheSecondGroup => [0, MEMBERS],
            Self::InTheCommonCells | Self::InTheLibrary => [MEMBERS, MEMBERS],
            Self::NothingToSee => [0, 0],
        }
    }
}

/// How many times the database is saved and restored before the changes.
#[derive(Clone, Copy, Debug)]
enum Trips {
    Once,
    /// Saved again after being restored, as a run that reuses a state and saves it does.
    Twice,
}

/// What the state is written in: text, which a test can read, or the binary format runs save in,
/// which counts the entries of a map before it reads them.
#[derive(Clone, Copy, Debug)]
enum Format {
    Json,
    MessagePack,
}

fn round_trip(db: &mut Db) -> (Db, String) {
    let saved = serde_json::to_string(&<dyn Database>::as_serialize(db))
        .unwrap_or_else(|error| panic!("serializing: {error}"));
    let mut restored = Db::default();
    <dyn Database>::deserialize(
        &mut restored,
        &mut serde_json::Deserializer::from_str(&saved),
    )
    .unwrap_or_else(|error| panic!("deserializing: {error}"));
    (restored, saved)
}

/// Saved and restored in `format`: the restored database and the number of bytes.
fn round_trip_in(db: &mut Db, format: Format) -> (Db, usize) {
    match format {
        Format::Json => {
            let (restored, saved) = round_trip(db);
            (restored, saved.len())
        }
        Format::MessagePack => {
            let saved = rmp_serde::to_vec(&<dyn Database>::as_serialize(db))
                .unwrap_or_else(|error| panic!("serializing: {error}"));
            let mut restored = Db::default();
            <dyn Database>::deserialize(
                &mut restored,
                &mut rmp_serde::Deserializer::from_read_ref(saved.as_slice()),
            )
            .unwrap_or_else(|error| panic!("deserializing: {error}"));
            (restored, saved.len())
        }
    }
}

/// Every way of saving and restoring that a test runs its changes after.
fn all_ways() -> Vec<(Format, Trips)> {
    [Format::Json, Format::MessagePack]
        .into_iter()
        .flat_map(|format| [Trips::Once, Trips::Twice].map(|trips| (format, trips)))
        .collect()
}

fn executions() -> [usize; 2] {
    [
        MEMBER_EXECUTIONS[0].load(Ordering::SeqCst),
        MEMBER_EXECUTIONS[1].load(Ordering::SeqCst),
    ]
}

/// A restored database, after `trips`, given each of `changes` in turn and asked for every query
/// after each: the queries of each group that computed again for each change, and the answers.
fn after(format: Format, trips: Trips, changes: &[Change]) -> (Vec<[usize; 2]>, Vec<[usize; 2]>) {
    let mut db = Db::default();
    let fixture = Fixture::new(&db);
    let first = fixture.ask_all(&db);
    let (mut restored, first_saved) = round_trip_in(&mut db, format);
    if let Trips::Twice = trips {
        // Asking for every query makes each one verified, so each is saved again.
        assert_eq!(fixture.ask_all(&restored), first);
        let (again, second_saved) = round_trip_in(&mut restored, format);
        // What a restored memo is saved as is what it was restored as: the sets stay sets.
        assert!(
            second_saved <= first_saved + first_saved / 10,
            "{format:?}: saved again, the state is {second_saved} bytes against {first_saved}"
        );
        restored = again;
    }
    // Asking before any change settles what is verified in this revision, which a later change
    // must not be hidden behind.
    assert_eq!(fixture.ask_all(&restored), first);
    let (mut computed, mut answers) = (Vec::new(), Vec::new());
    for (step, change) in changes.iter().enumerate() {
        let before = executions();
        match change {
            Change::InTheFirstGroup => {
                let cell = fixture.cells[0][CELLS / 2];
                cell.set_value(&mut restored).to(900_000 + step);
            }
            Change::InTheSecondGroup => {
                let cell = fixture.cells[1][CELLS / 3];
                cell.set_value(&mut restored).to(800_000 + step);
            }
            Change::InTheCommonCells => {
                let cell = fixture.cells[0][COMMON / 2];
                cell.set_value(&mut restored).to(600_000 + step);
            }
            Change::InTheLibrary => {
                Library::get(&restored)
                    .set_value(&mut restored)
                    .to(700_000 + step);
            }
            Change::NothingToSee => {
                Cell::new(&restored, 7);
            }
        }
        answers.push(fixture.ask_all(&restored));
        let now = executions();
        computed.push([now[0] - before[0], now[1] - before[1]]);
    }
    (computed, answers)
}

#[test]
fn what_a_change_reaches_through_a_shared_set_is_computed_again_and_nothing_else() {
    for (format, trips) in all_ways() {
        for change in [
            Change::InTheFirstGroup,
            Change::InTheSecondGroup,
            Change::InTheCommonCells,
            Change::InTheLibrary,
            Change::NothingToSee,
        ] {
            let (computed, _) = after(format, trips, &[change]);
            assert_eq!(
                computed,
                vec![change.reaches()],
                "{format:?}, {trips:?}, {change:?}: the queries that read what changed computed \
                 again, and only those"
            );
        }
    }
}

#[test]
fn a_change_after_another_is_noticed_by_what_was_verified_in_between() {
    // The memos the first change did not reach are verified again at the revision it made; the
    // second change must still reach them, and must not reach what the first already recomputed
    // and the second does not touch.
    for (format, trips) in all_ways() {
        let changes = [
            Change::InTheFirstGroup,
            Change::InTheSecondGroup,
            Change::NothingToSee,
            Change::InTheSecondGroup,
            Change::InTheCommonCells,
            Change::InTheLibrary,
        ];
        let (computed, answers) = after(format, trips, &changes);
        let expected: Vec<[usize; 2]> = changes.iter().map(|change| change.reaches()).collect();
        assert_eq!(
            computed, expected,
            "{format:?}, {trips:?}: each change reached what read it"
        );
        // The answers moved exactly when something that was read did.
        for (step, change) in changes.iter().enumerate() {
            if step > 0 {
                let moved = answers[step] != answers[step - 1];
                assert_eq!(
                    moved,
                    !matches!(change, Change::NothingToSee),
                    "{trips:?}, step {step} ({change:?}): the answers {} when they should {}",
                    if moved { "moved" } else { "stood" },
                    if moved { "stand" } else { "move" }
                );
            }
        }
    }
}

#[test]
fn memos_that_read_the_same_cells_do_not_each_take_a_copy_of_them() {
    let mut db = Db::default();
    let fixture = Fixture::new(&db);
    fixture.ask_all(&db);
    let before = salsa::edges_expanded();
    let _restored = round_trip(&mut db);
    let given = salsa::edges_expanded() - before;
    // A memo is given a reference to its group's set, and what it reads itself: not a copy of
    // the set's CELLS edges. Copies would be CELLS for each of the 2 x MEMBERS queries.
    let copies = (2 * MEMBERS * CELLS) as u64;
    assert!(
        given < copies / 10,
        "restoring gave memos {given} edges, where copies of their sets are {copies}"
    );
}

/// What `set_verifications` counted since `before`: references verified and sets checked.
fn set_work_since(before: (u64, u64, u64)) -> (u64, u64) {
    let now = salsa::set_verifications();
    (now.0 - before.0, now.1 - before.1)
}

#[test]
fn a_set_is_checked_once_for_all_the_memos_that_share_it_in_a_revision() {
    let mut db = Db::default();
    let fixture = Fixture::new(&db);
    fixture.ask_all(&db);
    let (mut restored, _) = round_trip(&mut db);

    // Nothing changed since the save, so no memo is verified by looking at its dependencies at
    // all: the sets are not checked.
    let before = salsa::set_verifications();
    fixture.ask_all(&restored);
    assert_eq!(
        set_work_since(before),
        (0, 0),
        "a restore nothing changed after is not verified through its sets"
    );

    // After a change, every memo has to be verified: each refers to its group's set, and the two
    // sets are checked once each between all the memos, and a change inside one is found.
    let cell = fixture.cells[0][CELLS / 2];
    cell.set_value(&mut restored).to(424_242);
    let before = salsa::set_verifications();
    fixture.ask_all(&restored);
    let (references, checks) = set_work_since(before);
    let memos = (2 * MEMBERS) as u64;
    assert!(
        references >= memos,
        "each of the {memos} memos had its set verified: {references}"
    );
    assert!(
        (1..=4).contains(&checks),
        "{references} references to 2 sets checked the sets {checks} times"
    );
}

/// Every ingredient's sets in a saved state, each as the runs it is saved as.
fn saved_sets(saved: &str) -> Vec<Vec<(u64, u64)>> {
    fn walk(value: &serde_json::Value, found: &mut Vec<Vec<(u64, u64)>>) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::Array(sets)) = map.get("sets") {
                    for set in sets {
                        let runs = set
                            .as_array()
                            .map(|runs| {
                                runs.iter()
                                    .filter_map(|run| {
                                        let run = run.as_array()?;
                                        Some((run.first()?.as_u64()?, run.get(1)?.as_u64()?))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        found.push(runs);
                    }
                }
                map.values().for_each(|value| walk(value, found));
            }
            serde_json::Value::Array(items) => items.iter().for_each(|value| walk(value, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(saved) {
        walk(&value, &mut found);
    }
    found
}

#[test]
fn what_the_memos_share_is_saved_as_runs_of_positions_not_as_edges() {
    let mut db = Db::default();
    let fixture = Fixture::new(&db);
    fixture.ask_all(&db);
    let (_, saved) = round_trip(&mut db);
    let sets = saved_sets(&saved);
    // The two groups' sets: the cells each reads, the first as one run and the second as two (the
    // common cells, then its own after the first group's), however many cells that is.
    let runs: Vec<usize> = sets.iter().map(Vec::len).collect();
    assert_eq!(
        runs,
        vec![1, 2],
        "the sets are saved as runs of positions: {sets:?}"
    );
    let edges: u64 = sets.iter().flatten().map(|(_, length)| length).sum();
    assert!(
        edges >= 2 * CELLS as u64,
        "the runs stand for the cells each group reads, {edges} edges"
    );
}
