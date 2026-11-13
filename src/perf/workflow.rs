//! Measure cordial on a real project: the pipeline `open` runs, cold, warm and
//! after one edit, with each stage's time read back from the gate ledger.

use super::{
    PerfEnvironment, PerfIdentity, PerfState, PerfSubject, RunRecord, Spread, StageTiming,
};
use crate::{CordialError, CordialResult, GateLedger, StoreLayout, project_slug_from_path};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use tracing::instrument;

/// One workflow: which cordial to run, how often, and what to edit for the
/// incremental state.
#[derive(Debug, Clone, derive_getters::Getters, derive_builder::Builder)]
#[builder(build_fn(error = "crate::error::CordialError"), setter(into))]
pub struct WorkflowRequest {
    /// The workflow's name, which becomes part of the record's subject.
    name: String,
    /// The cordial executable to run against the project.
    program: PathBuf,
    /// Runs per state; the median is reported.
    #[getter(copy)]
    repeat: u32,
    /// A file the incremental state edits (one comment line, restored after).
    #[builder(default)]
    edit: Option<PathBuf>,
}

/// What one run of the pipeline left behind.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StageSample {
    stages: BTreeMap<String, u64>,
    counters: BTreeMap<String, u64>,
}

impl StageSample {
    /// Read the stages and counters of the ledger's last run, plus the wall time
    /// of the process that produced it. Counters are exact for a given tree:
    /// the number of hooks each stage ran and their invocations.
    #[instrument(level = "debug", skip(ledger))]
    pub fn from_ledger(ledger: &GateLedger, process_nanos: u64) -> Self {
        let mut stages = BTreeMap::from([("process".to_owned(), process_nanos)]);
        let mut counters = BTreeMap::new();
        for timing in ledger.last_run_timings() {
            let nanos = u64::try_from(*timing.elapsed_nanos()).unwrap_or(u64::MAX);
            stages.insert(format!("{}/{}", timing.stage(), timing.id()), nanos);
            *counters.entry(format!("hooks.{}", timing.stage())).or_default() += 1;
            *counters.entry("invocations".to_owned()).or_default() += timing.invocations();
        }
        Self { stages, counters }
    }
}

/// Fold repeated samples into one timing per stage, and take the counters of
/// the last sample (they are the same on every repeat of an unchanged tree).
#[instrument(level = "debug", skip(samples))]
pub fn fold(samples: &[StageSample]) -> (Vec<StageTiming>, BTreeMap<String, u64>) {
    let mut by_stage: BTreeMap<&String, Vec<u64>> = BTreeMap::new();
    for sample in samples {
        for (name, nanos) in &sample.stages {
            by_stage.entry(name).or_default().push(*nanos);
        }
    }
    let stages = by_stage
        .into_iter()
        .filter_map(|(name, nanos)| {
            Some(StageTiming::new(
                name.clone(),
                Spread::from_samples(&nanos)?,
            ))
        })
        .collect();
    let counters = samples
        .last()
        .map(|sample| sample.counters.clone())
        .unwrap_or_default();
    (stages, counters)
}

/// Run `open` once against `root` with `work_home` as the store home, and read
/// the ledger it wrote.
#[instrument(level = "debug", skip(request), err(level = "warn"))]
fn run_once(request: &WorkflowRequest, root: &Path, work_home: &Path) -> CordialResult<StageSample> {
    let started = Instant::now();
    let status = Command::new(request.program())
        .arg("--project")
        .arg(root)
        .arg("--store-home")
        .arg(work_home)
        .arg("open")
        .env("CORDIAL_NO_PROGRESS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()?;
    let process = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    if !status.success() {
        return Err(CordialError::perf(
            root.to_path_buf(),
            format!("`open` exited with {:?}", status.code()),
        ));
    }
    let layout = StoreLayout::from_root(
        work_home.join(project_slug_from_path(root)),
        project_slug_from_path(root),
    );
    Ok(StageSample::from_ledger(
        &GateLedger::read(&layout)?,
        process,
    ))
}

/// Append one comment line to `file` and return the original bytes to restore.
#[instrument(level = "debug", skip(file), err(level = "warn"))]
fn edit_file(file: &Path, round: u32) -> CordialResult<Vec<u8>> {
    let original = std::fs::read(file)?;
    let mut edited = original.clone();
    edited.extend_from_slice(format!("\n// cordial perf edit {round}\n").as_bytes());
    std::fs::write(file, edited)?;
    Ok(original)
}

/// Measure the workflow: for each repeat, a cold run on an empty store, a warm
/// run straight after, and (when an edit file is named) an incremental run after
/// one edit. Returns one record per state. The edited file is always restored.
#[instrument(level = "debug", skip(root, work_home), err(level = "warn"))]
pub fn measure_workflow(
    root: &Path,
    work_home: &Path,
    request: &WorkflowRequest,
) -> CordialResult<Vec<RunRecord>> {
    let identity = PerfIdentity::capture(root)?;
    let environment = PerfEnvironment::capture(root);
    let slug_dir = work_home.join(project_slug_from_path(root));
    let (mut cold, mut warm, mut incremental) = (Vec::new(), Vec::new(), Vec::new());
    for round in 1..=request.repeat().max(1) {
        if slug_dir.exists() {
            std::fs::remove_dir_all(&slug_dir)?;
        }
        cold.push(run_once(request, root, work_home)?);
        warm.push(run_once(request, root, work_home)?);
        if let Some(file) = request.edit() {
            let path = root.join(file);
            let original = edit_file(&path, round)?;
            let sample = run_once(request, root, work_home);
            std::fs::write(&path, original)?;
            incremental.push(sample?);
        }
    }
    let mut records = Vec::new();
    for (state, samples) in [
        (PerfState::Cold, cold),
        (PerfState::Warm, warm),
        (PerfState::Incremental, incremental),
    ] {
        if samples.is_empty() {
            continue;
        }
        let (stages, counters) = fold(&samples);
        records.push(RunRecord::new(
            identity.clone(),
            environment.clone(),
            PerfSubject::workflow(request.name()),
            Some(state),
            stages,
            Vec::new(),
            counters,
            Vec::new(),
        ));
    }
    Ok(records)
}
