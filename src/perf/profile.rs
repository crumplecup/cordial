//! Per-test profiles: each test process writes the `hotpath` report of what it
//! ran, and this reads them back to say where a test's time went.

use super::{
    FunctionTiming, PerfSubject, ProfileUsage, RunRecord, RunRecordBuilder, Spread, StageTiming,
    TestTiming,
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
    /// What the profile says besides time: allocation, threads, locks, channels
    /// and reported values.
    #[new(default)]
    usage: ProfileUsage,
}

impl TestProfile {
    /// This profile with the non-timing sections it was read with.
    #[instrument(level = "trace", skip(self, usage))]
    pub fn with_usage(mut self, usage: ProfileUsage) -> Self {
        self.usage = usage;
        self
    }
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
#[hotpath::measure]
pub fn parse_report(text: &str) -> CordialResult<(u64, Vec<FunctionTiming>)> {
    let report: serde_json::Value = serde_json::from_str(text)?;
    Ok(timing_of(&report))
}

/// The timing section of a parsed report: its lifetime and its functions.
#[instrument(level = "debug", skip(report))]
#[hotpath::measure]
fn timing_of(report: &serde_json::Value) -> (u64, Vec<FunctionTiming>) {
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
    (wall, functions)
}

/// Every profile in `dir`, one per file named `{binary}${test}.json`.
#[instrument(level = "debug", err(level = "warn"))]
#[hotpath::measure]
pub fn read_profiles(dir: &Path) -> CordialResult<Vec<TestProfile>> {
    let mut profiles = Vec::new();
    for captured in read_captures(dir)? {
        if captured.status != crate::profiling::ReportStatus::Complete {
            continue;
        }
        let Some(manifest) = captured.manifest else {
            continue;
        };
        let (Some(binary), Some(name)) = (manifest.identity.binary, manifest.identity.test) else {
            continue;
        };
        let report: serde_json::Value = serde_json::from_reader(std::io::BufReader::new(
            std::fs::File::open(manifest.report)?,
        ))?;
        let (wall, functions) = timing_of(&report);
        profiles.push(
            TestProfile::new(binary, name, wall, functions)
                .with_usage(ProfileUsage::from_report(&report)),
        );
    }
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
        let report: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path)?)
            .map_err(|_| CordialError::perf(path.clone(), "not a hotpath report".to_owned()))?;
        let (wall, functions) = timing_of(&report);
        profiles.push(
            TestProfile::new(binary.to_owned(), name.to_owned(), wall, functions)
                .with_usage(ProfileUsage::from_report(&report)),
        );
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
#[hotpath::measure]
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
#[hotpath::measure]
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
#[hotpath::measure]
pub fn profiled(
    record: &RunRecord,
    set: &str,
    profiles: &[TestProfile],
) -> CordialResult<RunRecord> {
    let usage = ProfileUsage::merge(profiles.iter().map(TestProfile::usage));
    let mut counters = record.counters().clone();
    counters.insert("profiled".to_owned(), profiles.len() as u64);
    // The values that count things are exact, so the comparison gates on them.
    counters.extend(usage.exact_counters());
    RunRecordBuilder::default()
        .identity(record.identity().clone())
        .environment(record.environment().clone())
        .subject(PerfSubject::tests(format!("{set}+profile")))
        .state(record.state())
        .stages(record.stages().clone())
        .functions(aggregate(profiles))
        .counters(counters)
        .tests(attach(record.tests().clone(), profiles))
        .allocations(usage.allocations().clone())
        .threads(usage.threads().clone())
        .locks(usage.locks().clone())
        .channels(usage.channels().clone())
        .values(usage.values().clone())
        .build()
}

/// Durable expected-test inventory, populated from nextest's existing event stream.
/// No test initialization is needed for an expected missing report to remain visible.
pub struct TestExpectations {
    path: std::path::PathBuf,
    identities: Vec<crate::profiling::Identity>,
}

impl TestExpectations {
    /// A unique inventory file for one invocation/repeat.
    pub fn new(path: std::path::PathBuf) -> Self {
        Self {
            path,
            identities: Vec::new(),
        }
    }

    /// Register a started or completed test before the caller processes its result.
    pub fn observe(&mut self, line: &str, repeat: Option<u64>) -> CordialResult<()> {
        if !line.starts_with('{') {
            return Ok(());
        }
        let event: serde_json::Value = serde_json::from_str(line)?;
        if event["type"] != "test"
            || !matches!(event["event"].as_str(), Some("started" | "ok" | "failed"))
        {
            return Ok(());
        }
        let Some(name) = event["name"].as_str() else {
            return Ok(());
        };
        let (binary, test) = name.split_once('$').unwrap_or(("", name));
        let identity = crate::profiling::Identity {
            role: "test".to_owned(),
            binary: Some(binary.to_owned()),
            test: Some(test.to_owned()),
            repeat,
            boundary: "expected_test_entry".to_owned(),
            ..crate::profiling::Identity::default()
        };
        if !self.identities.contains(&identity) {
            self.identities.push(identity);
            crate::profiling::capture::write_json(&self.path, &self.identities)?;
        }
        Ok(())
    }
}

/// Read the shared process manifests for both workflows and test profiles.
/// Expected tests without capture and expected children without manifests survive.
pub fn read_captures(root: &Path) -> CordialResult<Vec<crate::profiling::RunReport>> {
    use crate::profiling::{Identity, ReportStatus, RunReport};
    let mut records = BTreeMap::new();
    let mut expected = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for record in crate::profiling::read_run(&directory)? {
            records.insert(record.process_id.clone(), record);
        }
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                pending.push(path);
            } else if path.to_string_lossy().ends_with(".expected.json") {
                let identities: Vec<Identity> = match serde_json::from_reader(
                    std::io::BufReader::new(std::fs::File::open(&path)?),
                ) {
                    Ok(identities) => identities,
                    Err(error) => {
                        let id = path.display().to_string();
                        records.insert(
                            id.clone(),
                            RunReport {
                                process_id: id,
                                expected: None,
                                manifest: None,
                                observation: None,
                                status: if error.is_eof() {
                                    ReportStatus::Truncated
                                } else {
                                    ReportStatus::Malformed
                                },
                            },
                        );
                        continue;
                    }
                };
                expected.extend(
                    identities
                        .into_iter()
                        .enumerate()
                        .map(|(index, identity)| (format!("{}:{index}", path.display()), identity)),
                );
            }
        }
    }
    for (id, identity) in expected {
        let captured = records.values().any(|record| {
            record.expected.as_ref().is_some_and(|actual| {
                actual.binary == identity.binary
                    && actual.test == identity.test
                    && actual.repeat == identity.repeat
            })
        });
        if !captured {
            records.insert(
                id.clone(),
                RunReport {
                    process_id: id,
                    expected: Some(identity),
                    manifest: None,
                    observation: None,
                    status: ReportStatus::Missing,
                },
            );
        }
    }
    Ok(records.into_values().collect())
}

/// Attach timing summaries and the complete process inventory from the same capture directory.
pub fn profiled_from(record: &RunRecord, set: &str, directory: &Path) -> CordialResult<RunRecord> {
    Ok(profiled(record, set, &read_profiles(directory)?)?
        .with_memory(super::read_memory(directory)?))
}
