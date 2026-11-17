//! The collectors that read only a file's tree run on every file before the steps that merge their
//! results, so the steps never parse a file for them.
//!
//! The steps used to sweep the files one collector at a time, each asking for its collector's
//! result. With the trees bounded, each sweep parsed again what the last had dropped. They now find
//! the results ready (one extraction per file, run in chunks by the priming), so what a step asks of
//! a file's tree is read from a memo. The metrics that tell the step a query ran for say it.

mod support {
    pub mod large_files;
}

use cordial::{
    CfgPlanning, CrateTarget, MetricsLayer, SavedState, SemanticCargoOptions, SourceDatabase,
    StoreLayout,
};
use miette::{IntoDiagnostic, Result};
use support::large_files::{crate_of_large_files, sources_of};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;

const FILES: usize = 160;

/// A projection of the crate with eviction on, and what the metrics say of the parses of a file:
/// all of them, those that ran for the extraction, and those that did when a collector's query asked
/// for it (the chain then has no `evaluate` in it: when the priming runs it, a worker's `evaluate` is
/// above the extraction).
fn parses(skip_priming: bool) -> Result<(u64, u64, u64, Vec<String>)> {
    cordial::init_tracing();
    let root = crate_of_large_files("onepass", FILES, 1500)?;
    let state = tempfile::tempdir().into_diagnostic()?;
    let saved = SavedState::at(state.path().join("analysis-state"));
    let subscriber = tracing_subscriber::registry()
        .with(MetricsLayer.with_filter(cordial::metrics_filter_with(true)));
    let _subscriber = tracing::subscriber::set_default(subscriber);
    // Evicting between the chunks of the priming bounds the trees held, as a run short of memory does.
    let options = SemanticCargoOptions::default()
        .with_cfg_planning(CfgPlanning::Explicit)
        .with_evicts_between_chunks(true)
        .with_skip_file_scan_priming(skip_priming);
    let mut session =
        SourceDatabase::load_saved(root.path(), &options, Some(saved)).into_diagnostic()?;
    session
        .project(
            &CrateTarget::new("onepass", root.path()),
            &sources_of(root.path()),
        )
        .into_diagnostic()?;

    let store = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(store.path().to_path_buf(), "onepass".to_owned());
    cordial::write_run_metrics(&layout).into_diagnostic()?;
    let metrics = cordial::read_run_metrics(&layout);
    let counted = |wanted: &dyn Fn(&str) -> bool| -> u64 {
        metrics
            .values()
            .iter()
            .filter(|(key, _)| {
                key.starts_with("steps.") && key.ends_with("> EditionedFileId::parse_")
            })
            .filter(|(key, _)| wanted(key))
            .map(|(_, count)| *count)
            .sum()
    };
    let all: Vec<String> = metrics
        .values()
        .iter()
        .filter(|(key, _)| key.starts_with("steps.") && key.ends_with("EditionedFileId::parse_"))
        .map(|(key, count)| format!("{key}={count}"))
        .collect();
    Ok((
        counted(&|_| true),
        counted(&|key| key.contains("extract_file")),
        counted(&|key| key.contains("extract_file <") && !key.contains("evaluate")),
        all,
    ))
}

/// With the pass, the steps find what the collectors found: no parse runs for one inside a step.
#[test]
fn the_steps_never_parse_a_file_for_the_collectors_that_read_only_its_tree() -> Result<()> {
    let (all, by_extraction, in_a_step, said) = parses(false)?;
    // That the metrics see the parses at all, or a zero below says nothing.
    assert!(
        all >= u64::try_from(FILES).unwrap_or(0),
        "the parses of {FILES} files were not seen: {said:#?}"
    );
    assert_eq!(
        in_a_step, 0,
        "{in_a_step} parses ran for a collector's query ({by_extraction} for the extraction in all)\n{said:#?}"
    );
    Ok(())
}

/// Without the pass, the first step that asks computes it, and parses: the check above can fail.
#[test]
fn without_the_pass_the_first_step_that_asks_parses() -> Result<()> {
    let (_, _, in_a_step, said) = parses(true)?;
    assert!(
        in_a_step > 0,
        "no parse ran for a collector's query either way\n{said:#?}"
    );
    Ok(())
}
