//! Per-test profiles: each test process writes the `hotpath` report of what it
//! ran, and this reads them back to say where a test's time went.

use super::{
    FunctionTiming, PerfSubject, RunRecord, RunRecordBuilder, Spread, StageTiming, TestTiming,
};
use crate::{CordialError, CordialResult};
use std::collections::BTreeMap;
use std::path::Path;
use tracing::instrument;

/// Functions whose time is setup, not the work a test checks: loading the
/// workspace into rust-analyzer (cargo metadata, build scripts, the database)
/// and projecting it into the shared IR. They are siblings, so their totals add.
/// A function matches when its name ends with one of these.
const FIXED_COST: &[&str] = &["::load_reporting", "::project_target"];

/// How many of a test's functions are kept on its record, slowest first.
const KEPT_PER_TEST: usize = 12;

/// One test's profile: its measured functions and the lifetime of its profile.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct TestProfile {
    binary: String,
    name: String,
    #[getter(copy)]
    wall_ns: u64,
    functions: Vec<FunctionTiming>,
}

/// A duration as `hotpath` prints it (`14.35 s`, `10.34 ms`, `14.97 µs`, `80 ns`).
#[instrument(level = "trace")]
pub fn parse_duration(text: &str) -> Option<u64> {
    let (number, unit) = text.trim().split_once(' ')?;
    let value: f64 = number.parse().ok()?;
    let scale = match unit {
        "s" => 1e9,
        "ms" => 1e6,
        "µs" | "us" => 1e3,
        "ns" => 1.0,
        _ => return None,
    };
    Some((value * scale).round() as u64)
}

/// The functions of one `hotpath` JSON report and the report's total lifetime.
#[instrument(level = "debug", skip(text), err(level = "warn"))]
pub fn parse_report(text: &str) -> CordialResult<(u64, Vec<FunctionTiming>)> {
    let report: serde_json::Value = serde_json::from_str(text)?;
    let timing = &report["functions_timing"];
    let wall = timing["total_elapsed_ns"].as_u64().unwrap_or(0);
    let functions = timing["data"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    Some(FunctionTiming::new(
                        row["name"].as_str()?.to_owned(),
                        row["calls"].as_u64()?,
                        parse_duration(row["total"].as_str()?)?,
                        0,
                        parse_duration(row["p95"].as_str()?)?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok((wall, functions))
}

/// Every profile in `dir`, one per file named `{binary}${test}.json`.
#[instrument(level = "debug", err(level = "warn"))]
pub fn read_profiles(dir: &Path) -> CordialResult<Vec<TestProfile>> {
    let mut profiles = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(stem) = path
            .extension()
            .filter(|extension| *extension == "json")
            .and(path.file_stem())
            .and_then(|stem| stem.to_str())
        else {
            continue;
        };
        let Some((binary, name)) = stem.split_once('$') else {
            continue;
        };
        let (wall, functions) = parse_report(&std::fs::read_to_string(&path)?)
            .map_err(|_| CordialError::perf(path.clone(), "not a hotpath report".to_owned()))?;
        profiles.push(TestProfile::new(
            binary.to_owned(),
            name.to_owned(),
            wall,
            functions,
        ));
    }
    Ok(profiles)
}

/// The setup time among `functions`: the outermost matching call of each.
#[instrument(level = "trace", skip(functions))]
pub fn fixed_ns(functions: &[FunctionTiming]) -> u64 {
    functions
        .iter()
        .filter(|function| {
            FIXED_COST
                .iter()
                .any(|name| function.name().ends_with(name))
        })
        .map(|function| function.total_ns())
        .sum()
}

/// Give each test the profile recorded for it: its slowest functions as stages
/// and its setup time as fixed cost. Tests without a profile are unchanged.
#[instrument(level = "debug", skip(tests, profiles))]
pub fn attach(tests: Vec<TestTiming>, profiles: &[TestProfile]) -> Vec<TestTiming> {
    let by_test: BTreeMap<(&str, &str), &TestProfile> = profiles
        .iter()
        .map(|profile| {
            (
                (profile.binary().as_str(), profile.name().as_str()),
                profile,
            )
        })
        .collect();
    tests
        .into_iter()
        .map(|test| {
            let Some(profile) = by_test.get(&(test.binary().as_str(), test.name().as_str())) else {
                return test;
            };
            let mut ranked: Vec<&FunctionTiming> = profile.functions().iter().collect();
            ranked.sort_by_key(|function| std::cmp::Reverse(function.total_ns()));
            let stages = ranked
                .into_iter()
                .take(KEPT_PER_TEST)
                .filter_map(|function| {
                    Some(StageTiming::new(
                        function.name().clone(),
                        Spread::from_samples(&[function.total_ns()])?,
                    ))
                })
                .collect();
            TestTiming::new(
                test.binary().clone(),
                test.name().clone(),
                test.wall(),
                stages,
                fixed_ns(profile.functions()),
                test.counters().clone(),
            )
        })
        .collect()
}

/// The functions of all profiles added together: calls and totals sum, the
/// slowest p95 wins.
#[instrument(level = "debug", skip(profiles))]
pub fn aggregate(profiles: &[TestProfile]) -> Vec<FunctionTiming> {
    let mut sums: BTreeMap<&str, (u64, u64, u64)> = BTreeMap::new();
    for function in profiles.iter().flat_map(|profile| profile.functions()) {
        let entry = sums.entry(function.name()).or_default();
        entry.0 += function.calls();
        entry.1 += function.total_ns();
        entry.2 = entry.2.max(function.p95_ns());
    }
    let mut merged: Vec<_> = sums
        .into_iter()
        .map(|(name, (calls, total, p95))| {
            FunctionTiming::new(name.to_owned(), calls, total, 0, p95)
        })
        .collect();
    merged.sort_by_key(|function| std::cmp::Reverse(function.total_ns()));
    merged
}

/// A timing record with the profiles joined in: each test's functions and fixed
/// cost, and the whole run's functions. It is a different subject from the
/// unprofiled set (`<set>+profile`), because profiling slows what it measures.
#[instrument(level = "debug", skip(record, profiles), err(level = "warn"))]
pub fn profiled(
    record: &RunRecord,
    set: &str,
    profiles: &[TestProfile],
) -> CordialResult<RunRecord> {
    let mut counters = record.counters().clone();
    counters.insert("profiled".to_owned(), profiles.len() as u64);
    RunRecordBuilder::default()
        .identity(record.identity().clone())
        .environment(record.environment().clone())
        .subject(PerfSubject::tests(format!("{set}+profile")))
        .state(record.state())
        .stages(record.stages().clone())
        .functions(aggregate(profiles))
        .counters(counters)
        .tests(attach(record.tests().clone(), profiles))
        .build()
}
