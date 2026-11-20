//! The run record: what one measured run wrote down.

use super::{
    ChannelUsage, FunctionAllocation, LockUsage, PerfEnvironment, PerfIdentity, Spread, ThreadUsage,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter, Result as FmtResult};
use tracing::instrument;

/// The record format version. A reader refuses any other version rather than
/// guess at its meaning.
pub const RECORD_SCHEMA: u32 = 1;

/// What kind of thing was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    /// A command sequence run on a target, such as `submit` on this repository.
    Workflow,
    /// A named set of tests.
    Tests,
}

/// The subject of a record: records compare only against the same subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct PerfSubject {
    #[getter(copy)]
    kind: SubjectKind,
    name: String,
}

impl PerfSubject {
    /// A workflow subject such as `cordial-submit`.
    #[instrument(level = "trace", skip(name))]
    pub fn workflow(name: impl Into<String>) -> Self {
        Self {
            kind: SubjectKind::Workflow,
            name: name.into(),
        }
    }

    /// A test-set subject such as `verus`.
    #[instrument(level = "trace", skip(name))]
    pub fn tests(name: impl Into<String>) -> Self {
        Self {
            kind: SubjectKind::Tests,
            name: name.into(),
        }
    }
}

impl Display for PerfSubject {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        let kind = match self.kind {
            SubjectKind::Workflow => "workflow",
            SubjectKind::Tests => "tests",
        };
        write!(formatter, "{kind}:{}", self.name)
    }
}

/// The state a workflow started from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerfState {
    /// Empty store and run cache.
    Cold,
    /// A run-cache hit on an unchanged tree.
    Warm,
    /// One named file edited since the last run.
    Incremental,
    /// A second edit after an earlier one, on whatever the first left behind.
    Followup,
}

impl Display for PerfState {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(match self {
            Self::Cold => "cold",
            Self::Warm => "warm",
            Self::Incremental => "incremental",
            Self::Followup => "followup",
        })
    }
}

/// Wall time of one pipeline stage.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct StageTiming {
    name: String,
    #[getter(copy)]
    time: Spread,
}

/// One measured function from the `hotpath` report.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct FunctionTiming {
    name: String,
    #[getter(copy)]
    calls: u64,
    #[getter(copy)]
    total_ns: u64,
    #[getter(copy)]
    self_ns: u64,
    #[getter(copy)]
    p95_ns: u64,
}

/// One test: its wall time, where that time went, and how much of it was setup.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct TestTiming {
    binary: String,
    name: String,
    #[getter(copy)]
    wall: Spread,
    stages: Vec<StageTiming>,
    #[getter(copy)]
    fixed_ns: u64,
    counters: BTreeMap<String, u64>,
}

impl TestTiming {
    /// The share of the test's median wall time spent on setup, from 0 to 1.
    #[instrument(level = "trace", skip(self))]
    pub fn fixed_share(&self) -> f64 {
        if self.wall.median() == 0 {
            return 0.0;
        }
        (self.fixed_ns as f64 / self.wall.median() as f64).min(1.0)
    }
}

/// Everything one measured run recorded.
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
pub struct RunRecord {
    /// The record format version; a reader refuses any it does not know.
    #[builder(default = "RECORD_SCHEMA")]
    #[getter(copy)]
    schema: u32,
    /// The commit that was measured.
    identity: PerfIdentity,
    /// The machine, software and dependencies the run happened on.
    environment: PerfEnvironment,
    /// What was measured.
    subject: PerfSubject,
    /// The state a workflow started from, when it has one.
    #[builder(default)]
    #[getter(copy)]
    state: Option<PerfState>,
    /// Wall time of each pipeline stage.
    #[builder(default)]
    stages: Vec<StageTiming>,
    /// Measured functions from the `hotpath` report.
    #[builder(default)]
    functions: Vec<FunctionTiming>,
    /// Exact counts of the run, such as hooks run per stage.
    #[builder(default)]
    counters: BTreeMap<String, u64>,
    /// Per-test timings, for test subjects.
    #[builder(default)]
    tests: Vec<TestTiming>,
    /// Bytes allocated per function, from a profile built with `hotpath-alloc`.
    #[builder(default)]
    #[serde(default)]
    allocations: Vec<FunctionAllocation>,
    /// Each thread's processor use and heap traffic.
    #[builder(default)]
    #[serde(default)]
    threads: Vec<ThreadUsage>,
    /// How long each named lock was waited for.
    #[builder(default)]
    #[serde(default)]
    locks: Vec<LockUsage>,
    /// How much crossed each named channel.
    #[builder(default)]
    #[serde(default)]
    channels: Vec<ChannelUsage>,
    /// The sizes the program reported: stored files and bytes, memoised results.
    #[builder(default)]
    #[serde(default)]
    values: BTreeMap<String, u64>,
    /// Exact allocation data per producer, including unavailable/incomplete captures.
    #[builder(default)]
    #[serde(default)]
    memory: Vec<super::MemoryCapture>,
    /// What the program said that cannot be added up: which kinds of query reached the most, why a
    /// fingerprint was volatile. The last of each.
    #[builder(default)]
    #[serde(default)]
    notes: BTreeMap<String, String>,
}

/// Lower-case letters, digits and hyphens only, so an id is a safe file name.
#[instrument(level = "trace")]
fn slug(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

impl RunRecord {
    /// The record's name in the store: sorts by time, then says what it is.
    #[instrument(level = "trace", skip(self))]
    pub fn id(&self) -> String {
        let sha: String = self.identity.sha().chars().take(8).collect();
        let state = self
            .state
            .map_or_else(String::new, |state| format!("-{state}"));
        format!(
            "{:010}-{}-{}{}",
            self.identity.recorded_at(),
            slug(&sha),
            slug(&self.subject.to_string()),
            state
        )
    }
}

impl RunRecord {
    /// Attach the shared memory import without merging process/thread/phase identities.
    pub fn with_memory(mut self, memory: Vec<super::MemoryCapture>) -> Self {
        self.memory = memory;
        self
    }
}
