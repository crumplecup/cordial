//! Which code a measurement was taken on, and when.

use crate::CordialResult;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::instrument;

/// The commit a record measures and the moment it was recorded.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_getters::Getters,
    derive_builder::Builder,
)]
#[builder(build_fn(error = "crate::error::CordialError"), setter(into))]
pub struct PerfIdentity {
    /// The commit that was measured.
    sha: String,
    /// Whether the tree had uncommitted changes.
    #[getter(copy)]
    dirty: bool,
    /// The branch checked out, if any.
    #[builder(default)]
    branch: Option<String>,
    /// Seconds since the Unix epoch when the record was made.
    #[getter(copy)]
    recorded_at: u64,
}

/// Standard output of `git` in `root`, or `None` when it fails.
#[instrument(level = "trace", skip(root))]
fn git(root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

impl PerfIdentity {
    /// Describe the checkout at `root`; outside a repository the sha is `unknown`.
    #[instrument(level = "debug", skip(root), err(level = "warn"))]
    #[hotpath::measure]
    pub fn capture(root: &Path) -> CordialResult<Self> {
        Ok(Self {
            sha: git(root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned()),
            dirty: git(root, &["status", "--porcelain"]).is_some_and(|text| !text.is_empty()),
            branch: git(root, &["branch", "--show-current"]).filter(|name| !name.is_empty()),
            recorded_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        })
    }
}
