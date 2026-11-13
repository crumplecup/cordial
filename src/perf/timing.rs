//! Measure how long each test in a set takes, repeated, as a run record.

use super::nextest::{NextestRequest, TestRun, parse_events, run};
use super::{
    PerfEnvironment, PerfIdentity, PerfSubject, RunRecord, RunRecordBuilder, Spread, StageTiming,
    TestTiming,
};
use crate::CordialResult;
use std::collections::BTreeMap;
use std::path::Path;
use tracing::instrument;

/// A timing run: which set, what selects it, and how many times to repeat it.
#[derive(Debug, Clone, derive_getters::Getters, derive_new::new)]
pub struct TimingRequest {
    set: String,
    nextest: NextestRequest,
    repeat: u32,
}

/// Fold repeated runs of the same tests into one timing per test, plus the
/// total time of each repeat. A test that failed in any repeat is counted.
#[instrument(level = "debug", skip(runs))]
#[hotpath::measure]
pub fn summarise(runs: &[Vec<TestRun>]) -> (Vec<TestTiming>, Vec<u64>, u64) {
    let mut samples: BTreeMap<(String, String), Vec<u64>> = BTreeMap::new();
    let mut failed: BTreeMap<(String, String), bool> = BTreeMap::new();
    let mut totals = Vec::new();
    for repeat in runs {
        totals.push(repeat.iter().map(|test| test.nanos()).sum());
        for test in repeat {
            let key = (test.binary().clone(), test.name().clone());
            samples.entry(key.clone()).or_default().push(test.nanos());
            *failed.entry(key).or_default() |= !test.passed();
        }
    }
    let failures = failed.values().filter(|failed| **failed).count() as u64;
    let timings = samples
        .into_iter()
        .filter_map(|((binary, name), nanos)| {
            Some(TestTiming::new(
                binary,
                name,
                Spread::from_samples(&nanos)?,
                Vec::new(),
                0,
                BTreeMap::new(),
            ))
        })
        .collect();
    (timings, totals, failures)
}

/// The record for repeats already measured, however they were obtained.
#[instrument(level = "debug", skip(root, runs), err(level = "warn"))]
#[hotpath::measure]
fn record_of(
    root: &Path,
    set: &str,
    threads: u32,
    runs: &[Vec<TestRun>],
) -> CordialResult<RunRecord> {
    let (tests, totals, failures) = summarise(runs);
    let stages = Spread::from_samples(&totals)
        .map(|spread| vec![StageTiming::new("tests_wall".to_owned(), spread)])
        .unwrap_or_default();
    let counters = BTreeMap::from([
        ("tests".to_owned(), tests.len() as u64),
        ("failed".to_owned(), failures),
        ("repeats".to_owned(), runs.len() as u64),
        ("threads".to_owned(), u64::from(threads)),
    ]);
    RunRecordBuilder::default()
        .identity(PerfIdentity::capture(root)?)
        .environment(PerfEnvironment::capture(root))
        .subject(PerfSubject::tests(set))
        .stages(stages)
        .counters(counters)
        .tests(tests)
        .build()
}

/// Run the set `repeat` times, keeping each repeat's raw events under `raw_dir`,
/// and record every test's duration.
#[instrument(level = "debug", skip(root, raw_dir), err(level = "warn"))]
#[hotpath::measure]
pub fn measure(root: &Path, request: &TimingRequest, raw_dir: &Path) -> CordialResult<RunRecord> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let mut runs = Vec::new();
    for repeat in 1..=(*request.repeat()).max(1) {
        let raw = raw_dir.join(format!("{stamp}-{}-{repeat}.jsonl", request.set()));
        runs.push(run(root, request.nextest(), &raw)?);
    }
    record_of(root, request.set(), request.nextest().threads(), &runs)
}

/// Build the record from raw event logs a run left behind, one file per repeat,
/// as when a long run was interrupted or its record was lost.
#[instrument(level = "debug", skip(root, files), err(level = "warn"))]
#[hotpath::measure]
pub fn measure_from(
    root: &Path,
    set: &str,
    threads: u32,
    files: &[std::path::PathBuf],
) -> CordialResult<RunRecord> {
    let mut runs = Vec::new();
    for file in files {
        runs.push(parse_events(&std::fs::read_to_string(file)?)?);
    }
    record_of(root, set, threads, &runs)
}
