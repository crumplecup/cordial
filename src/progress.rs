//! Runtime progress reporting hooks.

use std::sync::Arc;

use tracing::instrument;
/// User-facing progress reporter used by frontends such as the CLI.
///
/// Library callers get a no-op sink by default. Frontends that own a user
/// interface can install a sink on [`crate::SessionBuilder`] so the runtime can
/// report real stage and item progress without changing hook behavior.
pub trait ProgressSink: Send + Sync {
    /// Start an indeterminate task.
    fn spinner(&self, message: String) -> Box<dyn ProgressTask>;

    /// Start a counted task.
    fn bar(&self, message: String, len: u64) -> Box<dyn ProgressTask>;

    /// Show one user-facing status line.
    fn notice(&self, _: String) {}
}

/// One active progress task.
pub trait ProgressTask: Send + Sync {
    /// Replace the task's visible message.
    fn set_message(&self, message: String);

    /// Advance a counted task.
    fn inc(&self, delta: u64);

    /// Finish the task with a final message.
    fn finish(&self, message: String);
}

#[derive(Debug, Default)]
struct NoopProgress;

impl ProgressSink for NoopProgress {
    #[instrument(level = "trace", skip(self))]
    fn spinner(&self, _: String) -> Box<dyn ProgressTask> {
        Box::new(NoopProgressTask)
    }

    #[instrument(level = "trace", skip(self))]
    fn bar(&self, _: String, _: u64) -> Box<dyn ProgressTask> {
        Box::new(NoopProgressTask)
    }
}

#[derive(Debug)]
struct NoopProgressTask;

impl ProgressTask for NoopProgressTask {
    #[instrument(level = "trace", skip(self))]
    fn set_message(&self, _: String) {}

    #[instrument(level = "trace", skip(self))]
    fn inc(&self, _: u64) {}

    #[instrument(level = "trace", skip(self))]
    fn finish(&self, _: String) {}
}

static NOOP_PROGRESS: NoopProgress = NoopProgress;

/// Shared no-op progress reporter.
#[instrument(level = "debug")]
pub fn noop_progress() -> &'static dyn ProgressSink {
    &NOOP_PROGRESS
}

#[instrument(level = "debug")]
pub(crate) fn noop_progress_arc() -> Arc<dyn ProgressSink> {
    Arc::new(NoopProgress)
}
