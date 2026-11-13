//! Compare two run records: which counters moved exactly, which timings moved
//! by more than noise and tolerance, and which inputs differ between them.

use super::{ChangeKind, EnvironmentChange, RunRecord, Spread, format_nanos};
use std::collections::BTreeMap;
use std::fmt::Write;
use tracing::instrument;

/// The default share of its earlier median a timing must move to count (10%).
pub const DEFAULT_TOLERANCE: f64 = 0.10;

/// Counters that describe how a run was made, not what it did, so a difference
/// between them is not a regression.
const NOT_GATED: &[&str] = &["repeats", "threads", "profiled"];

/// What a comparison concludes about one quantity, or about the whole subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// It got slower, or an exact counter grew.
    Regressed,
    /// It got faster.
    Improved,
    /// Nothing moved by more than noise and tolerance.
    Unchanged,
}

impl std::fmt::Display for Verdict {
    #[instrument(level = "trace", skip(self, formatter))]
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Regressed => "regressed",
            Self::Improved => "improved",
            Self::Unchanged => "unchanged",
        })
    }
}

/// Whether `after` differs from `before` by more than both the tolerance (a
/// share of the earlier median) and the larger of the two spreads.
#[instrument(level = "trace")]
pub fn judge(before: Spread, after: Spread, tolerance: f64) -> Verdict {
    let moved = i128::from(after.median()) - i128::from(before.median());
    let size = moved.unsigned_abs();
    let tolerated = (tolerance * before.median() as f64) as u128;
    let noise = u128::from(before.mad().max(after.mad()));
    if size <= tolerated || size <= noise {
        Verdict::Unchanged
    } else if moved > 0 {
        Verdict::Regressed
    } else {
        Verdict::Improved
    }
}

/// One timing in both records.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct Delta {
    name: String,
    #[getter(copy)]
    before: Spread,
    #[getter(copy)]
    after: Spread,
    #[getter(copy)]
    verdict: Verdict,
}

impl Delta {
    /// Nanoseconds moved, positive when slower.
    #[instrument(level = "trace", skip(self))]
    pub fn moved(&self) -> i128 {
        i128::from(self.after.median()) - i128::from(self.before.median())
    }
}

/// One counter whose value differs.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct CounterDelta {
    name: String,
    #[getter(copy)]
    before: u64,
    #[getter(copy)]
    after: u64,
    /// Whether a rise fails the comparison.
    #[getter(copy)]
    gated: bool,
}

/// Everything two records say about each other.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct Comparison {
    subject: String,
    before_id: String,
    after_id: String,
    counters: Vec<CounterDelta>,
    stages: Vec<Delta>,
    functions: Vec<Delta>,
    tests: Vec<Delta>,
    changed_inputs: Vec<EnvironmentChange>,
    /// Setup share of all test time, before and after.
    fixed_share: Option<(f64, f64)>,
    #[getter(copy)]
    verdict: Verdict,
}

impl Comparison {
    /// Whether the machine differs, so time deltas may not be the code's doing.
    #[instrument(level = "trace", skip(self))]
    pub fn machine_changed(&self) -> bool {
        self.changed_inputs
            .iter()
            .any(|change| change.kind() == ChangeKind::Machine)
    }
}

/// Pair up the named timings of two records and judge each pair. A timing in
/// only one record has nothing to compare and is left out.
#[instrument(level = "trace", skip(before, after))]
fn pair(
    before: BTreeMap<String, Spread>,
    after: &BTreeMap<String, Spread>,
    tolerance: f64,
) -> Vec<Delta> {
    let mut deltas: Vec<Delta> = before
        .into_iter()
        .filter_map(|(name, earlier)| {
            let later = *after.get(&name)?;
            Some(Delta::new(
                name,
                earlier,
                later,
                judge(earlier, later, tolerance),
            ))
        })
        .collect();
    deltas.sort_by_key(|delta| std::cmp::Reverse(delta.moved().unsigned_abs()));
    deltas
}

#[instrument(level = "trace", skip(record))]
fn stages_of(record: &RunRecord) -> BTreeMap<String, Spread> {
    record
        .stages()
        .iter()
        .map(|stage| (stage.name().clone(), stage.time()))
        .collect()
}

#[instrument(level = "trace", skip(record))]
fn functions_of(record: &RunRecord) -> BTreeMap<String, Spread> {
    record
        .functions()
        .iter()
        .map(|function| (function.name().clone(), Spread::single(function.total_ns())))
        .collect()
}

#[instrument(level = "trace", skip(record))]
fn tests_of(record: &RunRecord) -> BTreeMap<String, Spread> {
    record
        .tests()
        .iter()
        .map(|test| (format!("{}::{}", test.binary(), test.name()), test.wall()))
        .collect()
}

#[instrument(level = "trace", skip(record))]
fn share_of(record: &RunRecord) -> Option<f64> {
    let wall: u64 = record.tests().iter().map(|test| test.wall().median()).sum();
    let fixed: u64 = record.tests().iter().map(|test| test.fixed_ns()).sum();
    (wall > 0 && fixed > 0).then(|| fixed as f64 / wall as f64)
}

/// Compare `after` against `before`. Any two records can be compared; what
/// differs between their environments is listed, never a reason to refuse.
#[instrument(level = "debug", skip(before, after))]
pub fn compare(before: &RunRecord, after: &RunRecord, tolerance: f64) -> Comparison {
    let mut counters = Vec::new();
    let names: std::collections::BTreeSet<&String> = before
        .counters()
        .keys()
        .chain(after.counters().keys())
        .collect();
    for name in names {
        let earlier = before.counters().get(name).copied().unwrap_or(0);
        let later = after.counters().get(name).copied().unwrap_or(0);
        if earlier != later {
            counters.push(CounterDelta::new(
                name.clone(),
                earlier,
                later,
                !NOT_GATED.contains(&name.as_str()),
            ));
        }
    }
    let stages = pair(stages_of(before), &stages_of(after), tolerance);
    let functions = pair(functions_of(before), &functions_of(after), tolerance);
    let tests = pair(tests_of(before), &tests_of(after), tolerance);
    let grew = counters
        .iter()
        .any(|counter| counter.gated() && counter.after() > counter.before());
    let shrank = counters
        .iter()
        .any(|counter| counter.gated() && counter.after() < counter.before());
    let verdicts: Vec<Verdict> = stages.iter().chain(&tests).map(Delta::verdict).collect();
    let verdict = if grew || verdicts.contains(&Verdict::Regressed) {
        Verdict::Regressed
    } else if shrank || verdicts.contains(&Verdict::Improved) {
        Verdict::Improved
    } else {
        Verdict::Unchanged
    };
    Comparison {
        subject: format!(
            "{}{}",
            after.subject(),
            after
                .state()
                .map_or_else(String::new, |state| format!(" {state}"))
        ),
        before_id: before.id(),
        after_id: after.id(),
        counters,
        stages,
        functions,
        tests,
        changed_inputs: after.environment().changes_from(before.environment()),
        fixed_share: share_of(before).zip(share_of(after)),
        verdict,
    }
}

/// The comparison as text: the verdict, exact counter changes, the timings that
/// moved most, and the inputs that differ. `top` bounds each list.
#[instrument(level = "debug", skip(comparison))]
pub fn render_comparison(comparison: &Comparison, top: usize) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}: {}\n  {} -> {}",
        comparison.subject(),
        comparison.verdict().to_string().to_uppercase(),
        comparison.before_id(),
        comparison.after_id()
    );
    if comparison.machine_changed() {
        let _ = writeln!(
            out,
            "  Caution: measured on different hardware; counters are exact, times may not be the code's doing."
        );
    }
    if !comparison.counters().is_empty() {
        let _ = writeln!(out, "\nCounters (exact):");
        for counter in comparison.counters() {
            let note = match (counter.gated(), counter.after() > counter.before()) {
                (true, true) => "  regressed",
                (true, false) => "  improved",
                (false, _) => "",
            };
            let _ = writeln!(
                out,
                "  {:>10} -> {:<10} {}{note}",
                counter.before(),
                counter.after(),
                counter.name()
            );
        }
    }
    for (title, deltas) in [
        ("Stages", comparison.stages()),
        ("Functions", comparison.functions()),
        ("Tests", comparison.tests()),
    ] {
        let moved: Vec<&Delta> = deltas
            .iter()
            .filter(|delta| delta.verdict() != Verdict::Unchanged)
            .collect();
        if moved.is_empty() {
            continue;
        }
        let _ = writeln!(
            out,
            "\n{title} that moved ({} of {}):",
            moved.len(),
            deltas.len()
        );
        for delta in moved.into_iter().take(top) {
            let sign = if delta.moved() >= 0 { '+' } else { '-' };
            let _ = writeln!(
                out,
                "  {:>9} -> {:<9} ({sign}{})  {}  {}",
                format_nanos(delta.before().median()),
                format_nanos(delta.after().median()),
                format_nanos(u64::try_from(delta.moved().unsigned_abs()).unwrap_or(u64::MAX)),
                delta.verdict(),
                delta.name()
            );
        }
    }
    if let Some((before, after)) = comparison.fixed_share() {
        let _ = writeln!(
            out,
            "\nSetup share of test time: {:.1}% -> {:.1}%",
            before * 100.0,
            after * 100.0
        );
    }
    if comparison.changed_inputs().is_empty() {
        let _ = writeln!(out, "\nInputs: unchanged.");
    } else {
        let _ = writeln!(out, "\nChanged inputs:");
        for change in comparison.changed_inputs() {
            let _ = writeln!(
                out,
                "  {:<10} {}: {} -> {}",
                format!("{:?}", change.kind()).to_lowercase(),
                change.name(),
                change.before(),
                change.after()
            );
        }
    }
    out
}
