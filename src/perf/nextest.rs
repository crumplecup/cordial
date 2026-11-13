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
    /// Extra environment for the test processes (a profile run names its output).
    #[new(default)]
    env: Vec<(String, String)>,
}

impl NextestRequest {
    /// The same request with one more environment variable for the tests.
    #[instrument(level = "trace", skip(self, value))]
    pub fn with_env(mut self, name: &str, value: String) -> Self {
        self.env.push((name.to_owned(), value));
        self
    }
}

/// The test binaries to build for `filter`, when it names nothing else.
///
/// Nextest applies a filterset after it has built, so `-E binary(x)` still
/// compiles every test binary in the repository, which for a new feature set is
/// most of an hour. A filter that is only `binary(name)` terms joined by `|` can
/// say the same thing to cargo with `--test name`, which builds just those. `None`
/// when the filter says more than that, or a name is not a test file here, and the
/// caller then builds everything because it cannot do better.
#[instrument(level = "debug", skip(root))]
#[hotpath::measure]
pub fn build_targets(root: &Path, filter: &str) -> Option<Vec<String>> {
    let mut targets = Vec::new();
    for term in filter.split('|') {
        let name = term
            .trim()
            .strip_prefix("binary(")?
            .strip_suffix(')')?
            .trim();
        let plain = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-');
        if !plain || !root.join("tests").join(format!("{name}.rs")).is_file() {
            return None;
        }
        targets.push(name.to_owned());
    }
    Some(targets)
}

/// Read the events nextest prints with `--message-format libtest-json-plus`.
/// Only tests that ran carry a duration; skipped and ignored ones are left out.
#[instrument(level = "debug", skip(output), err(level = "warn"))]
#[hotpath::measure]
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
#[hotpath::measure]
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
        .envs(request.env().iter().cloned())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if request.no_default_features {
        command.arg("--no-default-features");
    }
    match build_targets(root, request.filter()) {
        Some(targets) => {
            for target in targets {
                command.args(["--test", &target]);
            }
        }
        None => tracing::warn!(
            filter = request.filter(),
            "the filter is more than test binaries, so every test binary is built; \
             only the filtered tests run"
        ),
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
