//! Wall-clock timings for the hooks used by a completed gate.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::CordialResult;

/// The child timing pass uses this target instead of each etiquette's usual one.
#[cfg(any(
    feature = "rustdoc",
    feature = "doc_warnings",
    feature = "feature_warnings"
))]
#[instrument(level = "trace")]
pub(crate) fn timing_target_dir(default: std::path::PathBuf) -> std::path::PathBuf {
    std::env::var_os("CORDIAL_TIMINGS_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or(default)
}

/// Whether a child process is measuring build cost with isolated Cargo output.
#[cfg(feature = "rustdoc")]
#[instrument(level = "trace")]
pub(crate) fn is_timing_process() -> bool {
    std::env::var_os("CORDIAL_TIMINGS_TARGET_DIR").is_some()
}

/// Aggregated duration and invocation count for one hook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct HookTiming {
    stage: String,
    id: String,
    elapsed_nanos: u128,
    invocations: u64,
}

/// Collector for a run's hook calls, including repeated per-crate calls.
#[derive(Debug, Default)]
pub struct HookTimings(BTreeMap<(String, String), HookTiming>);

impl HookTimings {
    /// Execute a hook and record its elapsed time even when it fails.
    #[instrument(level = "trace", skip(self, action), err(level = "warn"))]
    pub fn measure<T>(
        &mut self,
        stage: &str,
        id: &str,
        action: impl FnOnce() -> CordialResult<T>,
    ) -> CordialResult<T> {
        let started = Instant::now();
        let result = action();
        self.record(stage, id, started.elapsed());
        result
    }

    /// Add an elapsed duration to one hook's aggregate.
    #[instrument(level = "trace", skip(self))]
    pub fn record(&mut self, stage: &str, id: &str, elapsed: Duration) {
        let key = (stage.to_string(), id.to_string());
        let entry = self.0.entry(key).or_insert_with(|| HookTiming {
            stage: stage.to_string(),
            id: id.to_string(),
            elapsed_nanos: 0,
            invocations: 0,
        });
        entry.elapsed_nanos = entry.elapsed_nanos.saturating_add(elapsed.as_nanos());
        entry.invocations = entry.invocations.saturating_add(1);
    }

    /// Duration recorded for a hook at one stage.
    #[instrument(level = "trace", skip(self))]
    pub fn nanos_for(&self, stage: &str, id: &str) -> u128 {
        self.0
            .get(&(stage.to_string(), id.to_string()))
            .map_or(0, |entry| entry.elapsed_nanos)
    }

    /// Sorted measurements for the run outcome.
    #[instrument(level = "trace", skip(self))]
    pub fn into_vec(self) -> Vec<HookTiming> {
        self.0.into_values().collect()
    }
}
