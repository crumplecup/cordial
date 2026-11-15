//! What a long run is doing, read from the events it already emits.
//!
//! Salsa logs every query it executes. Counting those, and keeping the name of the latest,
//! says whether a phase that shows no other change is working and on what; the process's
//! resident memory and the time since the run began complete the picture. The CLI shows the
//! line beside whatever the phase is called, so a run of minutes is never a silent one.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use tracing::instrument;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

/// Where Salsa reports each query it executes.
const EXECUTION_TARGET: &str = "salsa::function::execute";

/// The latest query's name is kept once in this many executions: it is a hint, and a
/// lock per event would cost more than the count it sits beside.
const NAME_EVERY: u64 = 256;

/// Counts of a run so far.
#[derive(Debug)]
pub struct LiveStatus {
    started: Instant,
    executed: AtomicU64,
    latest: Mutex<String>,
    phase: Mutex<String>,
}

impl LiveStatus {
    /// The process-wide status. The run's clock starts at the first call, which the
    /// subscriber's installation makes the start of the run.
    #[instrument(level = "trace")]
    pub fn global() -> &'static Self {
        static STATUS: OnceLock<LiveStatus> = OnceLock::new();
        STATUS.get_or_init(|| Self {
            started: Instant::now(),
            executed: AtomicU64::new(0),
            latest: Mutex::new(String::new()),
            phase: Mutex::new(String::new()),
        })
    }

    /// Say what the run is doing now, in the words of its progress display.
    #[instrument(level = "trace", skip(self, phase))]
    pub fn set_phase(&self, phase: &str) {
        if let Ok(mut current) = self.phase.lock() {
            phase.clone_into(&mut current);
        }
    }

    /// What the run was last said to be doing.
    #[instrument(level = "trace", skip(self))]
    pub fn phase(&self) -> String {
        self.phase
            .lock()
            .map(|phase| phase.clone())
            .unwrap_or_default()
    }

    /// Salsa queries executed so far.
    #[instrument(level = "trace", skip(self))]
    pub fn executed(&self) -> u64 {
        self.executed.load(Ordering::Relaxed)
    }

    /// One line: time, queries executed and the latest one's name, memory now and at peak.
    #[instrument(level = "trace", skip(self))]
    #[hotpath::measure(impl_type = "LiveStatus")]
    pub fn line(&self) -> String {
        let mut line = clock(self.started.elapsed().as_secs());
        let executed = self.executed();
        if executed > 0 {
            let _ = write!(line, " · {} queries", count(executed));
            if let Ok(latest) = self.latest.lock()
                && !latest.is_empty()
            {
                let _ = write!(line, " (latest {latest})");
            }
        }
        if let Some(memory) = crate::process_memory::sample() {
            let _ = write!(
                line,
                " · {} (peak {})",
                gibibytes(memory.resident_kib()),
                gibibytes(memory.peak_kib())
            );
        }
        line
    }

    #[instrument(level = "trace", skip(self, message))]
    fn executing(&self, message: &str) {
        let before = self.executed.fetch_add(1, Ordering::Relaxed);
        if before.is_multiple_of(NAME_EVERY)
            && let Some(name) = message.split('(').next()
            && let Ok(mut latest) = self.latest.lock()
        {
            name.clone_into(&mut latest);
        }
    }
}

/// Counts Salsa's query executions into [`LiveStatus::global`].
///
/// Install it with a filter that lets only Salsa's execution events through (see
/// [`live_filter`]), so every other event costs it nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct LiveLayer;

impl<S: tracing::Subscriber> Layer<S> for LiveLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() != EXECUTION_TARGET {
            return;
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        LiveStatus::global().executing(&message.0);
    }
}

/// The filter to give [`LiveLayer`]: Salsa's execution events and nothing else.
#[instrument(level = "trace")]
pub fn live_filter() -> tracing_subscriber::filter::Targets {
    tracing_subscriber::filter::Targets::new().with_target(EXECUTION_TARGET, tracing::Level::INFO)
}

struct Message(String);

impl tracing::field::Visit for Message {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

fn clock(seconds: u64) -> String {
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m{:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h{:02}m", seconds / 3600, seconds / 60 % 60),
    }
}

#[expect(clippy::cast_precision_loss, reason = "a display figure")]
fn count(number: u64) -> String {
    match number {
        0..10_000 => number.to_string(),
        10_000..1_000_000 => format!("{}k", number / 1000),
        _ => format!("{:.1}M", number as f64 / 1e6),
    }
}

#[expect(clippy::cast_precision_loss, reason = "a display figure")]
fn gibibytes(kibibytes: u64) -> String {
    format!("{:.1} GB", kibibytes as f64 / (1024.0 * 1024.0))
}
