// Which parts of the source cannot change what a cached analysis records.
//
// This file is read by `build.rs`, which digests every other file under `src/`
// into the run-cache key, and by `tests/run_cache_scope.rs`, which checks that
// nothing it leaves out is used by anything it keeps. It holds one constant and
// nothing else, so both can include it.

/// Paths under `src/`, a file or a directory with a trailing `/`, whose edits
/// leave a cached analysis valid. A file not listed here is part of the key: a
/// new file is analysis until someone shows it is not, which a test checks.
pub const OUTSIDE_THE_ANALYSIS: &[&str] = &[
    "main.rs",
    "cli.rs",
    "cli/",
    "tracing_init.rs",
    "tracing_init/",
    "host/protocol.rs",
    "host/transport.rs",
    "host/transport/",
    "perf/compare.rs",
    "perf/environment.rs",
    "perf/nextest.rs",
    "perf/profile.rs",
    "perf/record.rs",
    "perf/report.rs",
    "perf/spread.rs",
    "perf/store.rs",
    "perf/timing.rs",
    "perf/workflow.rs",
];
