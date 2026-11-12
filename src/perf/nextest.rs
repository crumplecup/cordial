//! Run a set of tests under nextest and read each test's duration.

use crate::{CordialError, CordialResult};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use tracing::instrument;

/// One test's measured run.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct TestRun {
    /// The test binary, as nextest names it (`crate::binary`).
    binary: String,
    name: String,
    #[getter(copy)]
    nanos: u64,
    #[getter(copy)]
    passed: bool,
}

/// What to run: a nextest filterset over the features a normal test run uses,
/// on this many test threads (one keeps tests from contending with each other).
#[derive(Debug, Clone, derive_getters::Getters, derive_new::new)]
pub struct NextestRequest {
    filter: String,
    features: String,
    no_default_features: bool,
    #[getter(copy)]
    threads: u32,
}

/// Read the events nextest prints with `--message-format libtest-json-plus`.
/// Only tests that ran carry a duration; skipped and ignored ones are left out.
#[instrument(level = "debug", skip(output), err(level = "warn"))]
pub fn parse_events(output: &str) -> CordialResult<Vec<TestRun>> {
    let mut runs = Vec::new();
    for line in output.lines().filter(|line| line.starts_with('{')) {
        let event: serde_json::Value = serde_json::from_str(line)?;
        let (Some("test"), Some(result)) = (
            event["type"].as_str(),
            event["event"]
                .as_str()
                .filter(|e| matches!(*e, "ok" | "failed")),
        ) else {
            continue;
        };
        let (Some(name), Some(seconds)) = (event["name"].as_str(), event["exec_time"].as_f64())
        else {
            continue;
        };
        let (binary, test) = name.split_once('$').unwrap_or(("", name));
        runs.push(TestRun::new(
            binary.to_owned(),
            test.to_owned(),
            (seconds * 1e9).round() as u64,
            result == "ok",
        ));
    }
    Ok(runs)
}

/// Run the requested tests once and return their durations. Nextest's events
/// are written to `raw` as they arrive, so a long run that is interrupted still
/// leaves everything it measured. Failing tests are returned too, marked failed;
/// a run that produced no test results at all (a build error, a filter matching
/// nothing) is an error.
#[instrument(level = "debug", skip(root), err(level = "warn"))]
pub fn run(root: &Path, request: &NextestRequest, raw: &Path) -> CordialResult<Vec<TestRun>> {
    if let Some(parent) = raw.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut log = std::fs::File::create(raw)?;
    let mut command = Command::new("cargo");
    command
        .current_dir(root)
        .env("NEXTEST_EXPERIMENTAL_LIBTEST_JSON", "1")
        .args(["nextest", "run", "--no-fail-fast"])
        .args(["--test-threads", &request.threads().max(1).to_string()])
        .args(["--message-format", "libtest-json-plus"])
        .args(["--features", request.features()])
        .args(["-E", request.filter()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if request.no_default_features {
        command.arg("--no-default-features");
    }
    let mut child = command.spawn()?;
    let mut events = String::new();
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let line = line?;
            writeln!(log, "{line}")?;
            events.push_str(&line);
            events.push('\n');
        }
    }
    let status = child.wait()?;
    let runs = parse_events(&events)?;
    if runs.is_empty() {
        return Err(CordialError::perf(
            root.to_path_buf(),
            format!(
                "nextest produced no test results for `{}` (exit {:?}); \
                 check the filter and that the tests build",
                request.filter(),
                status.code()
            ),
        ));
    }
    Ok(runs)
}
