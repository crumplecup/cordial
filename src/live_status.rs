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

/// How many queries a phase's report names.
const PHASE_TOP: usize = 12;

/// Counts of a run so far.
#[derive(Debug)]
pub struct LiveStatus {
    started: Instant,
    executed: AtomicU64,
    latest: Mutex<String>,
    phase: Mutex<String>,
    /// The queries executed since the phase began, by name.
    in_phase: Mutex<std::collections::HashMap<String, u64>>,
    /// The memos deep-verified and the dependencies checked, as of the last phase's end.
    verified: Mutex<(u64, u64)>,
    /// The restored sets verified, checked and walked, as of the last phase's end.
    sets: Mutex<(u64, u64, u64)>,
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
            in_phase: Mutex::new(std::collections::HashMap::new()),
            verified: Mutex::new((0, 0)),
            sets: Mutex::new((0, 0, 0)),
        })
    }

    /// Say what the run is doing now, in the words of its progress display.
    #[instrument(level = "trace", skip(self, phase))]
    pub fn set_phase(&self, phase: &str) {
        let Ok(mut current) = self.phase.lock() else {
            return;
        };
        if *current == phase {
            return;
        }
        let ended = std::mem::replace(&mut *current, phase.to_owned());
        drop(current);
        self.report_phase(&ended);
    }

    /// Say which queries the phase that ended executed, most first: where a run's time went,
    /// by the name of the work and not only its count.
    #[instrument(level = "trace", skip(self))]
    fn report_phase(&self, ended: &str) {
        let Ok(mut names) = self.in_phase.lock() else {
            return;
        };
        let mut counts: Vec<(String, u64)> = names.drain().collect();
        drop(names);
        // Memos re-validated in the phase: time spent that no executed query accounts for.
        let now = salsa::deep_verifications();
        let (memos, edges) = self
            .verified
            .lock()
            .map(|mut before| {
                let spent = (now.0 - before.0, now.1 - before.1);
                *before = now;
                spent
            })
            .unwrap_or_default();
        let now_sets = salsa::set_verifications();
        let (set_refs, set_checks, set_members) = self
            .sets
            .lock()
            .map(|mut before| {
                let spent = (
                    now_sets.0 - before.0,
                    now_sets.1 - before.1,
                    now_sets.2 - before.2,
                );
                *before = now_sets;
                spent
            })
            .unwrap_or_default();
        if counts.is_empty() && memos == 0 && set_refs == 0 {
            return;
        }
        counts.sort_by_key(|(name, count)| (std::cmp::Reverse(*count), name.clone()));
        let queries: u64 = counts.iter().map(|(_, count)| count).sum();
        let top = counts
            .iter()
            .take(PHASE_TOP)
            .map(|(name, count)| format!("{name}={count}"))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::info!(
            target: "cordial::phase_queries",
            phase = ended,
            queries,
            kinds = counts.len(),
            verified = memos,
            edges_checked = edges,
            set_refs,
            set_checks,
            set_members,
            top = %top,
            "queries executed in the phase"
        );
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
        if let Some(name) = message.split('(').next()
            && let Ok(mut names) = self.in_phase.lock()
        {
            *names.entry(name.to_owned()).or_default() += 1;
        }
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
