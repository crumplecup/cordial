//! Measure cordial on a real project: the pipeline `open` runs, cold, warm and
//! after one edit, with each stage's time read back from the gate ledger.

use super::{
    PerfEnvironment, PerfIdentity, PerfState, PerfSubject, RunRecord, RunRecordBuilder, Spread,
    StageTiming,
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
    /// What the run said about itself, as numbers, and the text of it that cannot be added up.
    metrics: crate::RunMetrics,
}

impl StageSample {
    /// Read the stages and counters of the ledger's last run, plus the wall time
    /// of the process that produced it. Counters are exact for a given tree:
    /// the number of hooks each stage ran and their invocations.
    ///
    /// `fresh` says the command rewrote the ledger. When it did not, the ledger
    /// holds an earlier run's timings, which are not this run's: only the process
    /// time is kept, and `pipeline_ran` is 0 so the difference is visible.
    #[instrument(level = "debug", skip(ledger))]
    #[hotpath::measure]
    pub fn from_ledger(
        ledger: &GateLedger,
        metrics: crate::RunMetrics,
        process_nanos: u64,
        fresh: bool,
    ) -> Self {
        let mut stages = BTreeMap::from([("process".to_owned(), process_nanos)]);
        let mut counters = BTreeMap::from([("pipeline_ran".to_owned(), u64::from(fresh))]);
        if !fresh {
            return Self {
                stages,
                counters,
                metrics: crate::RunMetrics::default(),
            };
        }
        for timing in ledger.last_run_timings() {
            let nanos = u64::try_from(*timing.elapsed_nanos()).unwrap_or(u64::MAX);
            stages.insert(format!("{}/{}", timing.stage(), timing.id()), nanos);
            *counters
                .entry(format!("hooks.{}", timing.stage()))
                .or_default() += 1;
            *counters.entry("invocations".to_owned()).or_default() += timing.invocations();
        }
        Self {
            stages,
            counters,
            metrics,
        }
    }
}

/// Fold repeated samples into one timing per stage, and take the counters of
/// the last sample (they are the same on every repeat of an unchanged tree).
#[instrument(level = "debug", skip(samples))]
#[hotpath::measure]
pub fn fold(
    samples: &[StageSample],
) -> (Vec<StageTiming>, BTreeMap<String, u64>, crate::RunMetrics) {
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
    let metrics = samples
        .last()
        .map(|sample| sample.metrics.clone())
        .unwrap_or_default();
    (stages, counters, metrics)
}

/// The command that runs the whole pipeline in each state. `open` builds a
/// fresh list from an empty store; once a list exists it reuses it while the
/// tree looks unchanged, so the later states use `submit`, which always checks
/// the tree.
#[derive(Debug, Clone, Copy)]
enum Step {
    Open,
    Submit,
}

impl Step {
    #[instrument(level = "trace", skip(self))]
    fn arguments(self) -> &'static [&'static str] {
        match self {
            Self::Open => &["open"],
            Self::Submit => &["submit", "--tier", "build"],
        }
    }
}

/// Run `step` once against `root` with `work_home` as the store home, and read
/// the ledger it wrote.
#[instrument(level = "debug", skip(request), err(level = "warn"))]
#[hotpath::measure]
fn run_once(
    request: &WorkflowRequest,
    root: &Path,
    work_home: &Path,
    step: Step,
) -> CordialResult<StageSample> {
    let ledger_path = work_home
        .join(project_slug_from_path(root))
        .join("ledger.json");
    let before = std::fs::read(&ledger_path).ok();
    let started = Instant::now();
    let status = Command::new(request.program())
        .arg("--project")
        .arg(root)
        .arg("--store-home")
        .arg(work_home)
        .args(step.arguments())
        .env("CORDIAL_NO_PROGRESS", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()?;
    let process = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    if !status.success() {
        return Err(CordialError::perf(
            root.to_path_buf(),
            format!(
                "`cordial {}` exited with {:?}",
                step.arguments().join(" "),
                status.code()
            ),
        ));
    }
    let layout = StoreLayout::from_root(
        work_home.join(project_slug_from_path(root)),
        project_slug_from_path(root),
    );
    let fresh = std::fs::read(&ledger_path).ok() != before;
    Ok(StageSample::from_ledger(
        &GateLedger::read(&layout)?,
        crate::run_metrics::read(&layout),
        process,
        fresh,
    ))
}

/// Edit `file`, run `submit`, and put the file back whatever happened.
#[instrument(level = "debug", skip(request), err(level = "warn"))]
#[hotpath::measure]
fn edited_run(
    request: &WorkflowRequest,
    root: &Path,
    work_home: &Path,
    file: &Path,
    round: u32,
) -> CordialResult<StageSample> {
    let path = root.join(file);
    let original = edit_file(&path, round)?;
    let sample = run_once(request, root, work_home, Step::Submit);
    std::fs::write(&path, original)?;
    sample
}

/// Append one comment line to `file` and return the original bytes to restore.
#[instrument(level = "debug", skip(file), err(level = "warn"))]
#[hotpath::measure]
fn edit_file(file: &Path, round: u32) -> CordialResult<Vec<u8>> {
    let original = std::fs::read(file)?;
    let mut edited = original.clone();
    edited.extend_from_slice(format!("\n// cordial perf edit {round}\n").as_bytes());
    std::fs::write(file, edited)?;
    Ok(original)
}

/// Measure the workflow: for each repeat, a cold `open` on an empty store, a warm
/// `submit` straight after, and (when an edit file is named) an incremental run after
/// one edit. Returns one record per state. The edited file is always restored.
#[instrument(level = "debug", skip(root, work_home), err(level = "warn"))]
#[hotpath::measure]
pub fn measure_workflow(
    root: &Path,
    work_home: &Path,
    request: &WorkflowRequest,
) -> CordialResult<Vec<RunRecord>> {
    let (records, failure) = measure_workflow_partial(root, work_home, request);
    failure.map_or(Ok(records), Err)
}

/// [`measure_workflow`], keeping what it measured when a run fails: the records of the states that
/// finished, and the failure, if there was one.
#[instrument(level = "debug", skip(root, work_home))]
#[hotpath::measure]
pub fn measure_workflow_partial(
    root: &Path,
    work_home: &Path,
    request: &WorkflowRequest,
) -> (Vec<RunRecord>, Option<CordialError>) {
    let (cold, warm, incremental, failure) = run_states(root, work_home, request);
    match records_of(root, request, cold, warm, incremental) {
        Ok(records) => (records, failure),
        Err(error) => (Vec::new(), failure.or(Some(error))),
    }
}

/// The samples of each state, and the failure that ended the runs, if one did. What finished
/// before a failure is kept: a workflow whose last state fails has still measured the first two.
type States = (
    Vec<StageSample>,
    Vec<StageSample>,
    Vec<StageSample>,
    Option<CordialError>,
);

/// Run each state for each repeat, and stop at the first run that fails.
#[instrument(level = "debug", skip_all)]
#[hotpath::measure]
fn run_states(root: &Path, work_home: &Path, request: &WorkflowRequest) -> States {
    let (mut cold, mut warm, mut incremental) = (Vec::new(), Vec::new(), Vec::new());
    let mut run = || -> CordialResult<()> {
        let slug_dir = work_home.join(project_slug_from_path(root));
        for round in 1..=request.repeat().max(1) {
            if slug_dir.exists() {
                std::fs::remove_dir_all(&slug_dir)?;
            }
            cold.push(run_once(request, root, work_home, Step::Open)?);
            warm.push(run_once(request, root, work_home, Step::Submit)?);
            if let Some(file) = request.edit() {
                incremental.push(edited_run(request, root, work_home, file, round)?);
            }
        }
        Ok(())
    };
    let failure = run().err();
    (cold, warm, incremental, failure)
}

/// One record for each state that has samples.
#[instrument(level = "debug", skip_all, err(level = "warn"))]
#[hotpath::measure]
fn records_of(
    root: &Path,
    request: &WorkflowRequest,
    cold: Vec<StageSample>,
    warm: Vec<StageSample>,
    incremental: Vec<StageSample>,
) -> CordialResult<Vec<RunRecord>> {
    let identity = PerfIdentity::capture(root)?;
    let environment = PerfEnvironment::capture(root);
    let name = request.name().clone();
    let mut records = Vec::new();
    for (state, samples) in [
        (PerfState::Cold, cold),
        (PerfState::Warm, warm),
        (PerfState::Incremental, incremental),
    ] {
        if samples.is_empty() {
            continue;
        }
        let (stages, counters, metrics) = fold(&samples);
        records.push(
            RunRecordBuilder::default()
                .identity(identity.clone())
                .environment(environment.clone())
                .subject(PerfSubject::workflow(name.clone()))
                .state(Some(state))
                .stages(stages)
                .counters(counters)
                .values(metrics.values().clone())
                .notes(metrics.notes().clone())
                .build()?,
        );
    }
    Ok(records)
}
