//! The program-wide pieces of `hotpath` profiling: what every binary that links
//! this library shares, so a profile covers the CLI and each test the same way.
//!
//! Timing needs only `#[hotpath::measure]` on the functions. Allocation needs one
//! counting allocator for the whole program, declared here, so a binary does not
//! declare its own (a second would not link). CLI, child and test owners all
//! build the report guard here and publish through the same capture lifecycle.

/// Hotpath's upstream allocator records traffic in allocation-enabled builds.
#[cfg(feature = "hotpath-alloc")]
#[global_allocator]
static ALLOCATOR: hotpath::CountingAllocator = hotpath::CountingAllocator::new();

/// The guard that records a profile and writes it to `path` when it drops. Without
/// the `hotpath` feature it records nothing and writes nothing.
///
/// Every profile is built here, whether it is the CLI's or a test's, so they carry
/// the same sections. No section is limited: functions, threads, locks, channels,
/// I/O and values all list every row they have, because a profile exists to be
/// read back by `cordial perf`, not skimmed. Which sections appear is hotpath's own
/// rule: those the program fed, plus functions and threads always. Upstream
/// HOTPATH environment overrides take precedence; the manifest records them.
#[tracing::instrument(level = "debug", skip(path))]
#[hotpath::measure]
pub(crate) fn guard(caller: &'static str, path: std::path::PathBuf) -> hotpath::HotpathGuard {
    let builder = hotpath::HotpathGuardBuilder::new(caller)
        .format(hotpath::Format::JsonPretty)
        .output_path(&path)
        .functions_limit(0)
        .channels_limit(0)
        .mutexes_limit(0)
        .rw_locks_limit(0)
        .io_limit(0)
        .threads_limit(0);
    builder.build()
}

pub(crate) mod capture;
mod coverage;
mod process;
pub use coverage::{Boundary, boundaries};
#[cfg(feature = "tracing")]
pub use coverage::{OperationCoverage, operation_coverage};

pub use capture::{
    Capture, Child, FinishReason, Identity, Manifest, ReportStatus, RunReport, read_run,
};
#[cfg(feature = "cli")]
pub(crate) use process::configure;
pub use process::{ProcessProfile, test_run_directory};
pub(crate) use process::{expect_child, finish, workflow_child};
