//! Run the feature powerset and collect warnings that depend on features.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::config::FeatureWarningsThresholds;
use crate::error::CordialResult;

use super::gate::FeatureSet;
use super::types::FeatureWarningRecord;

use tracing::instrument;

mod invoke;
mod parse;
mod records;

use invoke::{cargo_hack_available, run_cargo_hack};
pub use parse::{parse_cargo_hack_output, parse_cargo_hack_run};
pub use records::{failure_advice, records_from_run};

/// Where a warning fires: `(file, line, lint code)`.
type SiteKey = (String, u32, String);

/// What the powerset run saw at one site.
#[derive(Debug, Default, Clone)]
struct SiteHits {
    combos: BTreeSet<FeatureSet>,
    names: BTreeSet<String>,
    message: String,
}

/// What the powerset run saw at one compile-error site.
#[derive(Debug, Default, Clone)]
struct FailureHits {
    /// Descriptions of the failed combinations (`--no-default-features ...`).
    combos: BTreeSet<String>,
    message: String,
}

/// Parsed output of one `cargo hack check --message-format=json` run.
#[derive(Debug, Default, Clone)]
pub struct HackRun {
    combos: BTreeSet<FeatureSet>,
    sites: BTreeMap<SiteKey, SiteHits>,
    /// One description per invocation whose build failed.
    failed_combos: Vec<String>,
    failures: BTreeMap<SiteKey, FailureHits>,
    /// `cargo hack` itself failed without any build error to point at.
    hack_failure: Option<String>,
}

impl HackRun {
    /// Number of distinct feature combinations that compiled.
    #[instrument(level = "trace", skip(self))]
    pub fn combination_count(&self) -> usize {
        self.combos.len()
    }

    /// Number of invocations whose build failed.
    #[instrument(level = "trace", skip(self))]
    pub fn failed_combination_count(&self) -> usize {
        self.failed_combos.len()
    }

    /// Record that `cargo hack` exited unsuccessfully. Only meaningful when
    /// no failed build explains it (a dependency or manifest problem); the
    /// `error` lines of `stderr` become the finding's message.
    #[instrument(level = "trace", skip(self))]
    pub fn with_hack_failure(mut self, stderr: &str) -> Self {
        if self.failures.is_empty() {
            let lines: Vec<&str> = stderr
                .lines()
                .filter(|line| line.starts_with("error"))
                .collect();
            let message = if lines.is_empty() {
                "cargo hack exited unsuccessfully".to_string()
            } else {
                lines.join(" | ")
            };
            self.hack_failure = Some(message);
        }
        self
    }
}

/// Raw output of one `cargo hack` invocation.
#[derive(Debug)]
struct HackOutput {
    stdout: String,
    stderr: String,
    success: bool,
}

/// Scan a crate: run the powerset unless this package is skipped or
/// `cargo hack` is not installed.
///
/// `resolve_root` is the base a reported diagnostic path is joined against
/// (cargo reports paths relative to the workspace root), as in
/// `scan_crate_doc_warnings`.
#[instrument(level = "debug", skip(policy), err(level = "warn"))]
pub fn scan_crate_feature_warnings(
    crate_root: &Path,
    resolve_root: &Path,
    crate_name: &str,
    policy: &FeatureWarningsThresholds,
) -> CordialResult<Vec<FeatureWarningRecord>> {
    if policy.skip(crate_name) || !crate_root.join("Cargo.toml").is_file() {
        return Ok(Vec::new());
    }
    if !cargo_hack_available(crate_root) {
        tracing::debug!("cargo-hack not available; skipping feature warning scan");
        return Ok(Vec::new());
    }
    let output = run_cargo_hack(crate_root, crate_name, policy)?;
    let mut run = parse_cargo_hack_run(&output.stdout, &output.stderr, crate_name);
    if !output.success {
        run = run.with_hack_failure(&output.stderr);
    }
    let mut ignore: BTreeSet<String> = policy.exclude_features().iter().cloned().collect();
    ignore.insert("default".to_string());
    records_from_run(
        &run,
        resolve_root,
        policy.include_universal(),
        &ignore,
        policy.private_feature_threshold(),
    )
}
