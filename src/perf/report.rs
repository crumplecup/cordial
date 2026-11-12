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
