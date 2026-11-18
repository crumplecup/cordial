//! What the priming does when the run is nearing its memory limit.
//!
//! The watchdog stops a run that passes its limit. Before that, from 60% of it, the work gives back
//! what it can compute again and takes smaller steps: the workers take no new function once a few
//! are taken, the chunk ends, and the eviction runs before the rest. These tests ask for the
//! pressure (a run that really used that much would not be a test) and read what the priming says
//! of its chunks; with no pressure the same crate is primed in one chunk, which is what shows the
//! difference is the pressure's.
//!
//! Each test is its own process under nextest, which is what keeps the process-wide pressure and
//! counters from reaching the other.

use cordial::{
    CfgPlanning, CrateTarget, MetricsLayer, SavedState, SemanticCargoOptions, SemanticContextKey,
    SourceDatabase, StoreLayout, feel_pressure, read_run_metrics, write_run_metrics,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

/// Fewer functions than the least a chunk holds, so that without pressure they are one chunk.
const FUNCTIONS: usize = 40;

/// Whether the run feels pressure, and on which rung: what a test changes and nothing else.
#[derive(Clone, Copy)]
enum Pressure {
    Felt,
    Absent,
    At(cordial::Pressure),
}

/// What the priming of a crate of [`FUNCTIONS`] functions said of its chunks.
struct Chunks {
    /// How many chunks it took.
    taken: u64,
    /// How many functions waited for a later chunk, summed over the chunks.
    waited: u64,
    /// How many times it evicted between chunks.
    evictions: u64,
    /// The caps on what the queries that keep the most recently used keep, summed over those
    /// evictions (so one more than a cap's worth for each is what each eviction applied).
    caps: u64,
    /// How many of those evictions there were (the run's own, not each query's).
    eviction_events: u64,
    /// How many workers each part of the priming ran, summed, and how many parts there were.
    workers: u64,
    parts: u64,
    /// How many values of queries that are not persisted were dropped between chunks, and in how
    /// many rounds of dropping.
    dropped: u64,
    rounds: u64,
}

fn priming_under(pressure: Pressure) -> Result<Chunks> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='pressed'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library: String = (0..FUNCTIONS)
        .map(|index| format!("pub fn f{index}() -> usize {{ {index} }}\n"))
        .collect();
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
    let _subscriber = tracing::subscriber::set_default(subscriber);
    cordial::feel_pressure_at(match pressure {
        Pressure::Absent => cordial::Pressure::None,
        Pressure::Felt => cordial::Pressure::Soft,
        Pressure::At(level) => level,
    });
    let options = SemanticCargoOptions::default();
    let mut session = SourceDatabase::load_saved(root.path(), &options, None).into_diagnostic()?;
    session
        .project(
            &CrateTarget::new("pressed", root.path()),
            &[root.path().join("src/lib.rs")],
        )
        .into_diagnostic()?;
    feel_pressure(false);

    let store = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(store.path().to_path_buf(), "pressed".to_owned());
    write_run_metrics(&layout).into_diagnostic()?;
    let metrics = read_run_metrics(&layout);
    let sum = |wanted: &dyn Fn(&str) -> bool| -> u64 {
        metrics
            .values()
            .iter()
            .filter(|(key, _)| wanted(key))
            .map(|(_, count)| *count)
            .sum()
    };
    Ok(Chunks {
        taken: sum(&|key| key == "prime.events"),
        waited: sum(&|key| key == "prime.remaining"),
        evictions: sum(&|key| key.starts_with("eviction.prime_chunk.") && key.ends_with(".events")),
        caps: sum(&|key| key == "eviction.prime_chunk.cap"),
        eviction_events: sum(&|key| key == "eviction.prime_chunk.events"),
        workers: sum(&|key| key == "semantic.priming.workers"),
        parts: sum(&|key| key == "prime.events"),
        dropped: sum(&|key| key == "eviction.prime_chunk.dropped_regenerable"),
        rounds: sum(&|key| key == "eviction.prime_chunk.rounds"),
    })
}

/// Without pressure the functions are one chunk and nothing is evicted between chunks (the run
/// asked for no eviction): the baseline the next test is read against.
#[test]
fn without_pressure_the_functions_are_primed_in_one_chunk() -> Result<()> {
    let chunks = priming_under(Pressure::Absent)?;
    assert_eq!(chunks.waited, 0, "no function waited for a later chunk");
    assert_eq!(chunks.evictions, 0, "nothing was evicted between chunks");
    Ok(())
}

/// Under pressure the workers stop early, the rest waits for the next chunk, and the run evicts
/// between them.
#[test]
fn under_pressure_the_priming_takes_smaller_chunks_and_evicts_between_them() -> Result<()> {
    let chunks = priming_under(Pressure::Felt)?;
    assert!(
        chunks.waited > 0,
        "some functions waited for a later chunk; chunks taken: {}",
        chunks.taken
    );
    assert!(chunks.evictions > 0, "the run evicted between chunks");
    // Each eviction kept no more than the pressure's cap of the trees and expansions, and not what
    // rust-analyzer marks them for.
    assert!(chunks.eviction_events > 0);
    assert_eq!(
        chunks.caps,
        chunks.eviction_events * 16,
        "every eviction under pressure applied the pressure's cap"
    );
    Ok(())
}

/// The graph of the crate projected under `pressure`, with a saved state to keep the database's
/// persisted memos for, and how many values the run spilled by then (the process's count: one
/// test, one process, so the second run's is the first's and its own).
fn graph_under(root: &std::path::Path, pressure: Pressure) -> Result<(String, u64)> {
    let (graph, spilled, _) = run_under(root, pressure, &SemanticCargoOptions::default())?;
    Ok((graph, spilled))
}

/// [`graph_under`] with the options a run is made with, and how many values of queries that are not
/// persisted the run dropped at the ends of its stages.
fn run_under(
    root: &std::path::Path,
    pressure: Pressure,
    options: &SemanticCargoOptions,
) -> Result<(String, u64, u64)> {
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    cordial::feel_pressure_at(match pressure {
        Pressure::Absent => cordial::Pressure::None,
        Pressure::Felt => cordial::Pressure::Soft,
        Pressure::At(level) => level,
    });
    let graph = {
        let mut session =
            SourceDatabase::load_saved(root, options, Some(saved)).into_diagnostic()?;
        let graph = session
            .project(
                &CrateTarget::new("pressed", root),
                &[root.join("src/lib.rs")],
            )
            .into_diagnostic()?;
        format!("{graph:?}")
    };
    feel_pressure(false);
    // The file spilled values were in goes with the run's database.
    let left: Vec<_> = std::fs::read_dir(state.path().join("analysis-spill"))
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "the spill file outlived the run: {left:?}");
    let store = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(store.path().to_path_buf(), "pressed".to_owned());
    write_run_metrics(&layout).into_diagnostic()?;
    let metrics = read_run_metrics(&layout);
    let summed = |suffix: &str| -> u64 {
        metrics
            .values()
            .iter()
            .filter(|(key, _)| key.starts_with("eviction.") && key.ends_with(suffix))
            .map(|(_, count)| *count)
            .sum()
    };
    Ok((graph, summed(".spilled"), summed(".dropped_regenerable")))
}

/// Values given to the spill file and read back make no difference to what the run finds.
#[test]
fn spilled_values_are_read_back_and_the_graph_is_the_same() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='pressed'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    let library: String = (0..FUNCTIONS)
        .map(|index| format!("pub fn f{index}() -> usize {{ {index} }}\n"))
        .collect();
    std::fs::write(root.path().join("src/lib.rs"), library).into_diagnostic()?;
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
    let _subscriber = tracing::subscriber::set_default(subscriber);

    let (plain, spilled_plain) = graph_under(root.path(), Pressure::Absent)?;
    assert_eq!(spilled_plain, 0, "nothing is spilled without pressure");
    let (pressed, spilled) = graph_under(root.path(), Pressure::Felt)?;
    assert!(spilled > 0, "values were spilled under pressure");
    assert_eq!(
        in_order(&pressed),
        in_order(&plain),
        "the graph is the one without"
    );
    Ok(())
}

/// A graph's text in a form that does not depend on the order a hash map was printed in: what comes
/// before its indexes as it is, and the words of the indexes sorted (the braces and commas go where
/// the order puts them).
fn in_order(graph: &str) -> (String, Vec<String>) {
    let (nodes, indexes) = graph.split_once(" indexes: ").unwrap_or((graph, ""));
    let mut words: Vec<String> = indexes
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    words.sort();
    (nodes.to_owned(), words)
}

/// A run with a cfg context that is still near its limit after evicting and spilling drops what a
/// query computes again at the end of each stage, and what it finds is the same.
#[test]
fn a_context_run_under_pressure_drops_what_is_regenerable_and_finds_the_same_graph() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='pressed'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        root.path().join("src/lib.rs"),
        "pub fn ordinary() {}\n#[cfg(kani)]\npub fn harness() { ordinary(); }\npub fn plain() { ordinary(); }\n",
    )
    .into_diagnostic()?;
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![SemanticContextKey::enabling(["kani"])])
        .with_cfg_planning(CfgPlanning::Explicit);

    let (plain, _, dropped_plain) = run_under(root.path(), Pressure::Absent, &options)?;
    assert_eq!(dropped_plain, 0, "nothing is dropped without pressure");
    let (pressed, _, dropped) = run_under(root.path(), Pressure::Felt, &options)?;
    assert!(
        dropped > 0,
        "values were dropped at the ends of stages under pressure"
    );
    assert_eq!(
        in_order(&pressed),
        in_order(&plain),
        "and the graph is the one without"
    );
    Ok(())
}

/// What the priming does on each rung: all the workers, then a quarter of them, then one; and from
/// the second rung what is not persisted is dropped after each chunk.
#[test]
fn on_the_first_rung_the_priming_keeps_all_its_workers_and_drops_nothing_between_chunks()
-> Result<()> {
    let chunks = priming_under(Pressure::At(cordial::Pressure::Soft))?;
    assert!(chunks.parts > 0);
    assert_eq!(
        chunks.dropped, 0,
        "nothing that is not persisted is dropped between chunks"
    );
    if std::thread::available_parallelism().map_or(1, usize::from) > 1 {
        assert!(
            chunks.workers > chunks.parts,
            "more than one worker for a part"
        );
    }
    Ok(())
}

#[test]
fn on_the_second_rung_the_priming_has_a_quarter_of_the_workers_and_drops_between_chunks()
-> Result<()> {
    let chunks = priming_under(Pressure::At(cordial::Pressure::Hard))?;
    let available = std::thread::available_parallelism().map_or(1, usize::from);
    let quarter = u64::try_from((available / 4).max(1)).unwrap_or(1);
    assert!(chunks.parts > 0);
    assert!(
        chunks.workers <= chunks.parts * quarter,
        "no more than a quarter of the workers for a part: {} workers in {} parts of at most {quarter}",
        chunks.workers,
        chunks.parts
    );
    assert!(
        chunks.dropped > 0,
        "what is not persisted was dropped between chunks"
    );
    Ok(())
}

#[test]
fn on_the_last_rung_the_priming_has_one_worker_and_takes_one_function_at_a_time_if_it_must()
-> Result<()> {
    let chunks = priming_under(Pressure::At(cordial::Pressure::Severe))?;
    assert!(chunks.parts > 0);
    assert_eq!(chunks.workers, chunks.parts, "one worker for every part");
    assert!(
        chunks.dropped > 0,
        "and what is not persisted was dropped between chunks"
    );
    assert!(
        chunks.rounds * 2 < chunks.parts,
        "but not on every chunk: rounds that give back little are followed by longer waits ({} rounds in {} parts)",
        chunks.rounds,
        chunks.parts
    );
    Ok(())
}
