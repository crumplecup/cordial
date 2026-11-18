//! What the passes over cfg contexts and over the integration-test files parse.
//!
//! A context is a copy of the crate's files under another cfg. The copy has new file ids, so what
//! asks for its syntax parses the text again, once for each context. These tests say where those
//! parses are, by the step of the run that asked, so that the passes can be made to share them.

use cordial::{
    CfgPlanning, CrateIr, CrateTarget, IrView, MetricsLayer, SavedState, SemanticCargoOptions,
    SemanticContextKey, SourceDatabase, StoreLayout,
};
use miette::{IntoDiagnostic, Result};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

const LIBRARY: &str = "\
pub fn ordinary() {}

#[cfg(kani)]
pub fn harness() { ordinary(); }

#[cfg(not(kani))]
pub fn unless_kani() {}

pub fn plain() { ordinary(); }
";

static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

const TESTS: &str = "\
#[test]
fn calls_the_library() { gated::ordinary(); gated::plain(); }

#[test]
fn calls_it_again() { gated::plain(); }

#[test]
fn and_once_more() { gated::ordinary(); }

#[cfg(kani)]
#[test]
fn only_under_kani() { gated::ordinary(); }
";

/// The metrics of a projection of the crate with a context, as `(chain, parses)` of those that
/// parse a file, in a run with no saved state (`restored` is false) or one the first run saved.
fn parses_in_contexts(second_run: bool, with_tests: bool) -> Result<Vec<(String, u64)>> {
    Ok(run_in_contexts(second_run, with_tests)?.0)
}

/// The same, with the projection the last run made.
fn run_in_contexts(second_run: bool, with_tests: bool) -> Result<(Vec<(String, u64)>, CrateIr)> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    for dir in ["src", "tests"] {
        std::fs::create_dir_all(root.path().join(dir)).into_diagnostic()?;
    }
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='gated'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    if with_tests {
        std::fs::write(root.path().join("tests/t.rs"), TESTS).into_diagnostic()?;
    }
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let options = SemanticCargoOptions::default()
        .with_cfg_contexts(vec![SemanticContextKey::enabling(["kani"])])
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_evicts_between_chunks(true);
    let mut sources = vec![root.path().join("src/lib.rs")];
    if with_tests {
        sources.push(root.path().join("tests/t.rs"));
    }
    let runs = if second_run { 2 } else { 1 };
    // The counts are the process's, which the tests of this file share: one at a time, and what a
    // run adds is what it counts above what was counted before it.
    let _alone = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let snapshot = || -> Result<std::collections::BTreeMap<String, u64>> {
        let store = tempfile::tempdir().into_diagnostic()?;
        let layout = StoreLayout::from_root(store.path().to_path_buf(), "gated".to_owned());
        cordial::write_run_metrics(&layout).into_diagnostic()?;
        Ok(cordial::read_run_metrics(&layout)
            .values()
            .iter()
            .map(|(key, count)| (key.clone(), *count))
            .collect())
    };
    let mut found = Vec::new();
    let mut graph = None;
    let mut before = snapshot()?;
    for run in 0..runs {
        let subscriber = tracing_subscriber::registry()
            .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
        let _subscriber = tracing::subscriber::set_default(subscriber);
        let mut session = SourceDatabase::load_saved(root.path(), &options, Some(saved.clone()))
            .into_diagnostic()?;
        graph = Some(
            session
                .project(&CrateTarget::new("gated", root.path()), &sources)
                .into_diagnostic()?,
        );
        let now = snapshot()?;
        if run + 1 == runs {
            // What this run added to what the runs before it counted.
            found = now
                .iter()
                .filter_map(|(key, count)| {
                    let added = count - before.get(key).copied().unwrap_or(0);
                    (added > 0).then(|| (key.clone(), added))
                })
                .collect();
        }
        before = now;
    }
    Ok((found, graph.ok_or_else(|| miette::miette!("no run"))?))
}

/// The parses that ran for the passes over contexts, by the pass's step that asked: the chain from
/// the query that parsed to the step, without the frame every projection runs in.
fn by_pass(found: &[(String, u64)]) -> std::collections::BTreeMap<String, u64> {
    let mut steps = std::collections::BTreeMap::new();
    for (key, count) in found {
        let Some(chain) = key
            .strip_prefix("steps.")
            .and_then(|rest| rest.strip_suffix("> EditionedFileId::parse_"))
        else {
            continue;
        };
        let parts: Vec<&str> = chain.split(" < ").map(str::trim).collect();
        let Some(at) = parts
            .iter()
            .position(|part| matches!(*part, "harvest" | "read_leaves"))
        else {
            continue;
        };
        let step: Vec<&str> = parts[..=at]
            .iter()
            .copied()
            .filter(|part| *part != "inner_")
            .collect();
        *steps.entry(step.join(" < ")).or_default() += count;
    }
    steps
}

/// A run that starts from nothing parses each context's copy of a file for the functions in it,
/// once for each context: the passes cannot share a parse with the original, whose file id is
/// another. This names what is parsed so that taking the parses away shows.
#[test]
fn a_cold_run_parses_each_contexts_copy_of_the_files_for_the_passes() -> Result<()> {
    let steps = by_pass(&parses_in_contexts(false, true)?);
    let expected: std::collections::BTreeMap<String, u64> = [
        ("functions_in_copy < harvest".to_owned(), 2),
        ("functions_in_copy < read_leaves".to_owned(), 4),
    ]
    .into();
    assert_eq!(steps, expected, "the copies the passes parse");
    Ok(())
}

/// A restored run takes the functions of each copy and the gates of each original from the saved
/// state: the passes over contexts parse nothing and read no gates.
#[test]
fn a_restored_run_parses_nothing_for_the_passes_over_contexts() -> Result<()> {
    let found = parses_in_contexts(true, true)?;
    assert_eq!(by_pass(&found), std::collections::BTreeMap::new());
    let gates: u64 = found
        .iter()
        .filter(|(key, _)| {
            key.starts_with("steps.gates_in <")
                && key.contains("< harvest <")
                && key.ends_with("> gates_in")
        })
        .map(|(_, count)| *count)
        .sum();
    assert_eq!(gates, 0, "the gates were read again: {found:#?}");
    Ok(())
}

/// A leaf context after the first reads the functions of the test files that a gate it flips can
/// change, and those it is first to have, not each one again: the others have the facts they had.
/// The leaf functions are what they were, joined from the contexts.
#[test]
fn a_leaf_context_reads_only_what_its_gates_change() -> Result<()> {
    let (found, graph) = run_in_contexts(false, true)?;
    let count = |name: &str| {
        found
            .iter()
            .find(|(key, _)| key == &format!("semantic.leaves.{name}"))
            .map_or(0, |(_, count)| *count)
    };
    // Three functions under `test`, then the four of the context that also has `kani` on, of which
    // only the one gated on it is read again.
    assert_eq!(count("functions"), 7, "{found:#?}");
    assert_eq!(count("read"), 4, "{found:#?}");
    let leaves = graph.leaf_functions().into_diagnostic()?;
    assert_eq!(
        leaves.len(),
        4,
        "every test function is a leaf: {leaves:#?}"
    );
    assert!(
        leaves.iter().all(|leaf| !leaf.callees().is_empty()),
        "each calls the library: {leaves:#?}"
    );
    Ok(())
}
