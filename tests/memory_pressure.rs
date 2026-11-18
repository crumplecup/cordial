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
    CrateTarget, MetricsLayer, SemanticCargoOptions, SourceDatabase, StoreLayout, feel_pressure,
    read_run_metrics, write_run_metrics,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

/// Fewer functions than the least a chunk holds, so that without pressure they are one chunk.
const FUNCTIONS: usize = 40;

/// Whether the run feels pressure: what a test changes and nothing else.
#[derive(Clone, Copy)]
enum Pressure {
    Felt,
    Absent,
}

/// What the priming of a crate of [`FUNCTIONS`] functions said of its chunks.
struct Chunks {
    /// How many chunks it took.
    taken: u64,
    /// How many functions waited for a later chunk, summed over the chunks.
    waited: u64,
    /// How many times it evicted between chunks.
    evictions: u64,
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
    feel_pressure(matches!(pressure, Pressure::Felt));
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
    Ok(())
}
