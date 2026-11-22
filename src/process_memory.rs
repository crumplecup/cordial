//! Resident memory of this process, read for performance events.
//!
//! Tracing events carry these numbers next to their durations, so a run's log
//! shows the OS memory envelope beside the Hotpath profile. These samples do not
//! attribute allocations or prove which operation owns live heap. Linux reads
//! `/proc/self/status`; elsewhere no sample is available.

use tracing::instrument;

/// One reading of this process's memory, in kibibytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct MemorySample {
    /// Currently resident set size (`VmRSS`).
    #[getter(copy)]
    resident_kib: u64,
    /// Peak resident set size so far (`VmHWM`).
    #[getter(copy)]
    peak_kib: u64,
}

/// The process's current and peak resident memory, when the platform reports it.
#[instrument(level = "trace")]
pub fn sample() -> Option<MemorySample> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |name: &str| -> Option<u64> {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))?
            .trim_start_matches(':')
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    };
    Some(MemorySample::new(field("VmRSS")?, field("VmHWM")?))
}

/// What the machine has free to give this process and its children, in kibibytes
/// (`MemAvailable`): the headroom that the process's own resident size does not show.
#[instrument(level = "trace")]
pub fn available_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("MemAvailable:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Emit the current memory beside the phase that just finished, as a
/// `cordial::milestones` event, so a run's log and its metrics show where memory grew and when.
/// One event per milestone: the run's metrics add up the numbers of events with the same name.
#[instrument(level = "debug")]
#[hotpath::measure]
pub(crate) fn log(phase: &str) {
    if let Some(memory) = sample() {
        tracing::debug!(
            target: "cordial::milestones",
            phase,
            resident_kib = memory.resident_kib(),
            peak_kib = memory.peak_kib(),
            available_kib = available_kib().unwrap_or(0),
            elapsed_ms = crate::LiveStatus::global().elapsed_ms(),
            "memory"
        );
    }
}

/// Logs this process's memory every second while a long phase runs.
///
/// A phase that grows without bound never reaches its end-of-phase sample; this
/// shows the envelope as it changes, labelled with the active phase. Concurrent
/// operations can also affect that envelope.
#[derive(Debug)]
pub(crate) struct Watch {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Watch {
    /// Start sampling for `phase` until the returned value is dropped.
    #[instrument(level = "debug")]
    #[hotpath::measure]
    pub(crate) fn start(phase: String) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        let spawned = std::thread::Builder::new()
            .name("cordial-memory-watch".to_owned())
            .spawn(move || {
                let mut elapsed = 0u64;
                while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    elapsed += 250;
                    if elapsed.is_multiple_of(1000)
                        && let Some(memory) = sample()
                    {
                        tracing::debug!(
                            target: "cordial::metrics",
                            phase = phase.as_str(),
                            seconds = elapsed / 1000,
                            resident_kib = memory.resident_kib(),
                            peak_kib = memory.peak_kib(),
                            "memory watch"
                        );
                    }
                }
            });
        let handle = match spawned {
            Ok(handle) => Some(handle),
            Err(error) => {
                // The run goes on without the climb being logged.
                tracing::warn!(error = ?error, "could not start the memory watch");
                None
            }
        };
        Self { stop, handle }
    }
}

impl Drop for Watch {
    #[instrument(level = "trace", skip(self))]
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            // A panicked sampler only loses its own log lines.
            let _ = handle.join();
        }
    }
}
