//! A line per run: every `open` and `submit` appends what it cost to a CSV in
//! the store, so how the tool performs accumulates without anyone measuring.

use crate::host::RunNotes;
use crate::{CordialResult, HookTiming};
use std::io::Write;
use std::path::PathBuf;
use tracing::instrument;

/// The columns, in order. Append only: a reader finds a column by name.
pub const COLUMNS: &[&str] = &[
    "at",
    "commit",
    "dirty",
    "mode",
    "loaded",
    "kept",
    "reloaded",
    "files_changed",
    "cache_hits",
    "findings",
    "wall_s",
    "load_and_probe_s",
    "source_ir_s",
    "call_graph_s",
    "source_facts_s",
    "enrich_s",
    "assess_s",
    "render_s",
    "stopped_after",
    "cache",
];

/// The log of this store's runs.
#[instrument(level = "trace", skip(store))]
pub fn path(store: &crate::StoreLayout) -> PathBuf {
    store.perf_dir().join("runs.csv")
}

/// Seconds spent in the hooks of `stage` whose id starts with `prefix`.
#[instrument(level = "trace", skip(timings))]
fn seconds(timings: &[HookTiming], stage: &str, prefix: &str) -> f64 {
    timings
        .iter()
        .filter(|timing| timing.stage() == stage && timing.id().starts_with(prefix))
        .map(|timing| timing.elapsed_nanos().to_owned() as f64 / 1e9)
        .sum()
}

/// What a run produced, as the log needs it.
#[derive(Debug, Clone, derive_getters::Getters, derive_new::new)]
pub struct RunFacts {
    /// The hooks' times.
    timings: Vec<HookTiming>,
    /// The run's wall time in seconds.
    wall_secs: f64,
    /// How many findings it produced.
    findings: usize,
    /// The stage it was asked to stop after, if it was stopped.
    stopped_after: Option<String>,
}

/// One run as a CSV row (no newline), matching [`COLUMNS`].
#[instrument(level = "debug", skip(facts, notes))]
#[hotpath::measure]
pub fn row(at: u64, commit: &str, dirty: bool, facts: &RunFacts, notes: RunNotes) -> String {
    let timings = facts.timings();
    let mode = if crate::host::keeping() {
        "session"
    } else {
        "process"
    };
    let fields = [
        at.to_string(),
        commit.replace(',', ""),
        u8::from(dirty).to_string(),
        mode.to_owned(),
        notes.loaded().to_string(),
        notes.kept().to_string(),
        notes.reloaded().to_string(),
        notes.files_changed().to_string(),
        notes.cache_hits().to_string(),
        facts.findings().to_string(),
        format!("{:.3}", facts.wall_secs()),
        format!("{:.3}", seconds(timings, "phase", "load_and_probe")),
        format!("{:.3}", seconds(timings, "pipeline", "source_ir")),
        format!("{:.3}", seconds(timings, "pipeline", "call_graph")),
        format!("{:.3}", seconds(timings, "pipeline", "source_facts")),
        format!("{:.3}", seconds(timings, "enrich", "")),
        format!("{:.3}", seconds(timings, "assess", "")),
        format!("{:.3}", seconds(timings, "phase", "render_and_write")),
        facts.stopped_after().clone().unwrap_or_default(),
        // Commas separate the columns, so the events are joined by semicolons.
        notes.events().join(" | ").replace(',', ";"),
    ];
    fields.join(",")
}

/// Append this run's row to the store's log, writing the header first when the
/// log is new. A log that cannot be written never fails the run it describes.
#[instrument(level = "debug", skip(store, facts, notes))]
#[hotpath::measure]
pub fn record(
    store: &crate::StoreLayout,
    project_root: &std::path::Path,
    facts: &RunFacts,
    notes: RunNotes,
) {
    let result = (|| -> CordialResult<()> {
        let identity = super::PerfIdentity::capture(project_root)?;
        let commit: String = identity.sha().chars().take(8).collect();
        let line = row(
            identity.recorded_at(),
            &commit,
            identity.dirty(),
            facts,
            notes,
        );
        let file = path(store);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let new = std::fs::metadata(&file).is_ok_and(|meta| meta.len() > 0);
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)?;
        if !new {
            writeln!(log, "{}", COLUMNS.join(","))?;
        }
        writeln!(log, "{line}")?;
        Ok(())
    })();
    if let Err(error) = result {
        tracing::warn!(error = ?error, "could not append to the run log");
    }
}
