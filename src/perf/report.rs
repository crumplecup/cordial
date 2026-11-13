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

/// A byte count in the unit that reads best, in powers of 1024.
#[instrument(level = "trace")]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// The slowest tests and the per-binary totals of a timing record, `top` tests long.
#[instrument(level = "debug", skip(record))]
#[hotpath::measure]
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
#[hotpath::measure]
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
#[hotpath::measure]
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
    out.push_str(&render_usage(record, top));
    out
}

/// The sections of a profile besides time: what was allocated, what was stored,
/// which locks were waited for and what each thread did. A section the profile
/// did not carry is left out.
#[instrument(level = "debug", skip(record))]
#[hotpath::measure]
pub fn render_usage(record: &RunRecord, top: usize) -> String {
    let mut out = String::new();
    if !record.allocations().is_empty() {
        let _ = writeln!(out, "\nFunctions by bytes allocated:");
        for item in record.allocations().iter().take(top) {
            let _ = writeln!(
                out,
                "  {:>9}  {:>8} calls  {}",
                format_bytes(item.total_bytes()),
                item.calls(),
                item.name()
            );
        }
    }
    if !record.values().is_empty() {
        // The largest first; the record keeps all of them.
        let mut values: Vec<(&String, &u64)> = record.values().iter().collect();
        values.sort_by_key(|(_, value)| std::cmp::Reverse(**value));
        let _ = writeln!(
            out,
            "\nStorage and other values (largest {} of {}):",
            top.min(values.len()),
            values.len()
        );
        for (key, value) in values.into_iter().take(top) {
            let shown = if key.ends_with(".bytes") {
                format_bytes(*value)
            } else {
                value.to_string()
            };
            let _ = writeln!(out, "  {shown:>9}  {key}");
        }
    }
    if !record.locks().is_empty() {
        let _ = writeln!(out, "\nLocks:");
        for lock in record.locks() {
            let _ = writeln!(
                out,
                "  {:>9} taken  wait avg {:>9}  p95 {:>9}  {}",
                lock.count(),
                format_nanos(lock.wait_avg_ns()),
                format_nanos(lock.wait_p95_ns()),
                lock.label()
            );
        }
    }
    if !record.channels().is_empty() {
        let _ = writeln!(out, "\nChannels:");
        for channel in record.channels() {
            let _ = writeln!(
                out,
                "  {:>9} sent  {:>9} received  {}",
                channel.sent(),
                channel.received(),
                channel.label()
            );
        }
    }
    if record
        .threads()
        .iter()
        .any(|thread| thread.alloc_bytes() > 0)
    {
        let _ = writeln!(out, "\nThreads by bytes allocated:");
        let mut threads: Vec<_> = record.threads().iter().collect();
        threads.sort_by_key(|thread| std::cmp::Reverse(thread.alloc_bytes()));
        for thread in threads.into_iter().take(top) {
            let _ = writeln!(
                out,
                "  {:>9} allocated  {:>9} freed  cpu peak {:>5.1}%  {}",
                format_bytes(thread.alloc_bytes()),
                format_bytes(thread.dealloc_bytes()),
                thread.cpu_peak_tenths() as f64 / 10.0,
                thread.name()
            );
        }
    }
    out
}
