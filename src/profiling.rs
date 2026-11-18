//! The program-wide pieces of `hotpath` profiling: what every binary that links
//! this library shares, so a profile covers the CLI and each test the same way.
//!
//! Timing needs only `#[hotpath::measure]` on the functions. Allocation needs one
//! counting allocator for the whole program, declared here, so a binary does not
//! declare its own (a second would not link). The guard that writes a profile
//! belongs here too once there is one place that builds it.

/// The program's one allocator when allocation profiling is on: it counts the
/// bytes and the allocations of every function hotpath measures, and of every
/// thread. Without the feature the program keeps the system allocator.
#[cfg(feature = "hotpath-alloc")]
#[global_allocator]
static ALLOCATOR: hotpath::CountingAllocator = hotpath::CountingAllocator::with(std::alloc::System);

/// The guard that records a profile and writes it to `path` when it drops. Without
/// the `hotpath` feature it records nothing and writes nothing.
///
/// Every profile is built here, whether it is the CLI's or a test's, so they carry
/// the same sections. No section is limited: functions, threads, locks, channels,
/// I/O and values all list every row they have, because a profile exists to be
/// read back by `cordial perf`, not skimmed. Which sections appear is hotpath's own
/// rule: those the program fed, plus functions and threads always.
#[tracing::instrument(level = "debug", skip(path))]
#[hotpath::measure]
pub(crate) fn guard(caller: &'static str, path: std::path::PathBuf) -> hotpath::HotpathGuard {
    hotpath::HotpathGuardBuilder::new(caller)
        .format(hotpath::Format::JsonPretty)
        .output_path(path)
        .functions_limit(0)
        .channels_limit(0)
        .mutexes_limit(0)
        .rw_locks_limit(0)
        .io_limit(0)
        .threads_limit(0)
        .build()
}

/// The guard of the run's profile, kept where the memory watchdog can write it: it ends the process
/// with `exit`, which runs no destructor, and a run killed for memory is the one whose profile is
/// wanted.
#[cfg(feature = "hotpath")]
static HELD: std::sync::Mutex<Option<hotpath::HotpathGuard>> = std::sync::Mutex::new(None);

/// Keep `guard` until [`finish`] or the end of the run.
#[cfg(feature = "hotpath")]
#[tracing::instrument(level = "debug", skip(guard))]
pub(crate) fn hold(guard: hotpath::HotpathGuard) {
    if let Ok(mut held) = HELD.lock() {
        *held = Some(guard);
    }
}

/// Write the profile now, if one is kept (the report is written when the guard drops). Nothing
/// without the `hotpath` feature.
#[tracing::instrument(level = "debug")]
pub(crate) fn finish() {
    #[cfg(feature = "hotpath")]
    {
        let guard = HELD.lock().ok().and_then(|mut held| held.take());
        drop(guard);
    }
}

/// What the command's scope holds: on drop, writes the profile if it was not written already.
#[cfg(feature = "hotpath")]
#[derive(Debug)]
pub(crate) struct Held;

#[cfg(feature = "hotpath")]
impl Drop for Held {
    fn drop(&mut self) {
        finish();
    }
}
