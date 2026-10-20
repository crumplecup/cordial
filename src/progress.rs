//! Runtime progress reporting hooks.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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

/// Line-oriented progress for frontends whose stderr is not a terminal.
///
/// Each task writes start, operation change, and finish lines. A running task
/// writes a heartbeat every ten seconds. All lines go to the supplied writer,
/// so a CLI can keep its result on stdout.
pub struct LineProgress {
    output: Arc<Mutex<Box<dyn Write + Send>>>,
    heartbeat_interval: Duration,
}

impl LineProgress {
    /// Write progress lines to stderr with a ten-second heartbeat.
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        Self::with_writer(io::stderr(), Duration::from_secs(10))
    }

    /// Write progress lines to `writer` at the given heartbeat interval.
    ///
    /// This also lets embedding frontends capture progress independently of
    /// their result stream. A zero interval is treated as one millisecond.
    #[instrument(level = "trace", skip(writer))]
    pub fn with_writer(writer: impl Write + Send + 'static, heartbeat_interval: Duration) -> Self {
        Self {
            output: Arc::new(Mutex::new(Box::new(writer))),
            heartbeat_interval: heartbeat_interval.max(Duration::from_millis(1)),
        }
    }

    #[instrument(level = "debug", skip(self))]
    fn start(&self, message: String, total: Option<u64>) -> Box<dyn ProgressTask> {
        write_progress_line(&self.output, &format!("Starting: {message}"));
        let state = Arc::new(Mutex::new(LineTaskState {
            message,
            position: 0,
            total,
        }));
        let (stop, stopped) = mpsc::channel();
        let task_state = Arc::clone(&state);
        let output = Arc::clone(&self.output);
        let interval = self.heartbeat_interval;
        let worker = std::thread::spawn(move || {
            let started = Instant::now();
            while matches!(
                stopped.recv_timeout(interval),
                Err(RecvTimeoutError::Timeout)
            ) {
                let Ok(state) = task_state.lock() else {
                    break;
                };
                let count = state
                    .total
                    .map(|total| format!(" {}/{total}", state.position))
                    .unwrap_or_default();
                write_progress_line(
                    &output,
                    &format!(
                        "still working: {}{count} ({}s)",
                        state.message,
                        started.elapsed().as_secs()
                    ),
                );
            }
        });
        Box::new(LineTask {
            state,
            output: Arc::clone(&self.output),
            stop,
            worker: Mutex::new(Some(worker)),
            finished: AtomicBool::new(false),
        })
    }
}

impl Default for LineProgress {
    #[instrument(level = "debug")]
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressSink for LineProgress {
    #[instrument(level = "trace", skip(self))]
    fn spinner(&self, message: String) -> Box<dyn ProgressTask> {
        self.start(message, None)
    }

    #[instrument(level = "trace", skip(self))]
    fn bar(&self, message: String, len: u64) -> Box<dyn ProgressTask> {
        self.start(message, Some(len))
    }

    #[instrument(level = "trace", skip(self))]
    fn notice(&self, message: String) {
        write_progress_line(&self.output, &message);
    }
}

struct LineTaskState {
    message: String,
    position: u64,
    total: Option<u64>,
}

struct LineTask {
    state: Arc<Mutex<LineTaskState>>,
    output: Arc<Mutex<Box<dyn Write + Send>>>,
    stop: Sender<()>,
    worker: Mutex<Option<JoinHandle<()>>>,
    finished: AtomicBool,
}

impl LineTask {
    #[instrument(level = "debug", skip(self))]
    fn stop_heartbeat(&self) {
        let _ = self.stop.send(());
        if let Ok(mut worker) = self.worker.lock()
            && let Some(worker) = worker.take()
        {
            let _ = worker.join();
        }
    }
}

impl ProgressTask for LineTask {
    #[instrument(level = "trace", skip(self))]
    fn set_message(&self, message: String) {
        if let Ok(mut state) = self.state.lock()
            && state.message != message
        {
            let count = state
                .total
                .map(|total| format!(" {}/{total}", state.position))
                .unwrap_or_default();
            write_progress_line(&self.output, &format!("Working: {message}{count}"));
            state.message = message;
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn inc(&self, delta: u64) {
        if let Ok(mut state) = self.state.lock() {
            state.position = state.position.saturating_add(delta);
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn finish(&self, message: String) {
        if !self.finished.swap(true, Ordering::AcqRel) {
            self.stop_heartbeat();
            write_progress_line(&self.output, &format!("Finished: {message}"));
        }
    }
}

impl Drop for LineTask {
    #[instrument(level = "trace", skip(self))]
    fn drop(&mut self) {
        self.stop_heartbeat();
    }
}

#[instrument(level = "info", skip(output))]
fn write_progress_line(output: &Mutex<Box<dyn Write + Send>>, line: &str) {
    if let Ok(mut output) = output.lock() {
        let _ = writeln!(output, "{line}");
        let _ = output.flush();
    }
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
