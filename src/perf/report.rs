//! Render a test-timing record as a ranking.

use super::{RunRecord, TestTiming};
use std::collections::BTreeMap;
use std::fmt::Write;
use tracing::instrument;

/// A duration in the unit that reads best: seconds, milliseconds or microseconds.
#[instrument(level = "trace")]
pub fn format_nanos(nanos: u64) -> String {
    match nanos {
        n if n >= 1_000_000_000 => format!("{:.2}s", n as f64 / 1e9),
        n if n >= 1_000_000 => format!("{:.1}ms", n as f64 / 1e6),
        n => format!("{}us", n / 1_000),
    }
}

/// The slowest tests and the per-binary totals of a timing record, `top` tests long.
#[instrument(level = "debug", skip(record))]
pub fn render_timing(record: &RunRecord, top: usize) -> String {
    let mut out = String::new();
    let total: u64 = record.tests().iter().map(|test| test.wall().median()).sum();
    let _ = writeln!(
        out,
        "{} at {} ({} repeat(s), {} tests, {} failed): {} of test time",
        record.subject(),
        record.identity().sha().chars().take(8).collect::<String>(),
        record.counters().get("repeats").copied().unwrap_or(0),
        record.tests().len(),
        record.counters().get("failed").copied().unwrap_or(0),
        format_nanos(total)
    );
    let mut binaries: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    for test in record.tests() {
        let entry = binaries.entry(test.binary()).or_default();
        entry.0 += 1;
        entry.1 += test.wall().median();
    }
    let mut ranked: Vec<_> = binaries.into_iter().collect();
    ranked.sort_by_key(|(_, (_, nanos))| std::cmp::Reverse(*nanos));
    let _ = writeln!(out, "\nBy binary:");
    for (binary, (count, nanos)) in ranked {
        let share = if total == 0 {
            0.0
        } else {
            nanos as f64 * 100.0 / total as f64
        };
        let _ = writeln!(
            out,
            "  {:>9}  {share:>5.1}%  {count:>4} tests  {binary}",
            format_nanos(nanos)
        );
    }
    let mut tests: Vec<&TestTiming> = record.tests().iter().collect();
    tests.sort_by_key(|test| std::cmp::Reverse(test.wall().median()));
    let _ = writeln!(out, "\nSlowest {} tests:", top.min(tests.len()));
    for test in tests.into_iter().take(top) {
        let _ = writeln!(
            out,
            "  {:>9}  ±{:<8}  {}::{}",
            format_nanos(test.wall().median()),
            format_nanos(test.wall().mad()),
            test.binary(),
            test.name()
        );
    }
    out
}

/// The stages of a workflow record, slowest first, with the counters beneath.
#[instrument(level = "debug", skip(record))]
pub fn render_stages(record: &RunRecord, top: usize) -> String {
    let mut out = String::new();
    let state = record
        .state()
        .map_or_else(String::new, |state| format!(" {state}"));
    let _ = writeln!(
        out,
        "{}{state} at {} ({} stages)",
        record.subject(),
        record.identity().sha().chars().take(8).collect::<String>(),
        record.stages().len()
    );
    let mut stages: Vec<_> = record.stages().iter().collect();
    stages.sort_by_key(|stage| std::cmp::Reverse(stage.time().median()));
    for stage in stages.into_iter().take(top) {
        let _ = writeln!(
            out,
            "  {:>9}  ±{:<8}  {}",
            format_nanos(stage.time().median()),
            format_nanos(stage.time().mad()),
            stage.name()
        );
    }
    for (name, value) in record.counters() {
        let _ = writeln!(out, "  {value:>9}  {name}");
    }
    out
}

/// Where a profiled run's time went: the setup share of all test time, the tests
/// with the most setup, and the functions that took the most in total.
#[instrument(level = "debug", skip(record))]
pub fn render_profile(record: &RunRecord, top: usize) -> String {
    let mut out = String::new();
    let wall: u64 = record.tests().iter().map(|test| test.wall().median()).sum();
    let fixed: u64 = record.tests().iter().map(TestTiming::fixed_ns).sum();
    let share = if wall == 0 {
        0.0
    } else {
        fixed as f64 * 100.0 / wall as f64
    };
    let _ = writeln!(
        out,
        "{}: {} profiled of {} tests; setup {} of {} ({share:.1}%)",
        record.subject(),
        record.counters().get("profiled").copied().unwrap_or(0),
        record.tests().len(),
        format_nanos(fixed),
        format_nanos(wall)
    );
    let mut tests: Vec<&TestTiming> = record.tests().iter().collect();
    tests.sort_by_key(|test| std::cmp::Reverse(test.fixed_ns()));
    let _ = writeln!(out, "\nMost setup:");
    for test in tests.into_iter().take(top) {
        let _ = writeln!(
            out,
            "  {:>9}  {:>5.1}% of {:>9}  {}::{}",
            format_nanos(test.fixed_ns()),
            test.fixed_share() * 100.0,
            format_nanos(test.wall().median()),
            test.binary(),
            test.name()
        );
    }
    let _ = writeln!(out, "\nFunctions by total time:");
    for function in record.functions().iter().take(top) {
        let _ = writeln!(
            out,
            "  {:>9}  {:>8} calls  p95 {:>9}  {}",
            format_nanos(function.total_ns()),
            function.calls(),
            format_nanos(function.p95_ns()),
            function.name()
        );
    }
    out
}
