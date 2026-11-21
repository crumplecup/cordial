//! Compare two run records: which counters moved exactly, which timings moved
//! by more than noise and tolerance, and which inputs differ between them.

use super::{ChangeKind, EnvironmentChange, RunRecord, Spread, format_bytes, format_nanos};
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
    /// Per-operation traffic in compatible process/repeat/mode boundaries.
    /// Legacy records retain approximate formatted-byte trends.
    allocations: Vec<Delta>,
    /// Why allocation comparisons are partial, unavailable, or incompatible.
    memory_issues: Vec<String>,
    /// Complete attribution requires every process and boundary to be verified.
    #[getter(copy)]
    memory_complete: bool,
    /// Stored and held sizes in bytes (the store's entries, a context's copy, the
    /// database's memos). They trend; the counts beside them (files, crates,
    /// memos) are exact and gate through the counters.
    storage: Vec<Delta>,
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
fn allocations_of(record: &RunRecord) -> BTreeMap<String, Spread> {
    record
        .allocations()
        .iter()
        .map(|item| (item.name().clone(), Spread::single(item.total_bytes())))
        .collect()
}

#[instrument(level = "trace", skip(record))]
fn storage_of(record: &RunRecord) -> BTreeMap<String, Spread> {
    record
        .values()
        .iter()
        .filter(|(key, _)| key.ends_with(".bytes"))
        .map(|(key, value)| (key.clone(), Spread::single(*value)))
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
#[hotpath::measure]
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
    let (earlier_memory, later_memory, memory_issues) = memory_traffic(before, after);
    let allocations = pair(earlier_memory, &later_memory, tolerance);
    let memory_complete = !before.memory().is_empty()
        && !after.memory().is_empty()
        && memory_issues.is_empty()
        && before
            .memory()
            .iter()
            .chain(after.memory())
            .all(super::MemoryCapture::attribution_complete);
    let storage = pair(storage_of(before), &storage_of(after), tolerance);
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
        allocations,
        memory_issues,
        memory_complete,
        storage,
        changed_inputs: after.environment().changes_from(before.environment()),
        fixed_share: share_of(before).zip(share_of(after)),
        verdict,
    }
}

/// The comparison as text: the verdict, exact counter changes, the timings that
/// moved most, and the inputs that differ. `top` bounds each list.
#[instrument(level = "debug", skip(comparison))]
#[hotpath::measure]
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
    let _ = writeln!(
        out,
        "  Memory attribution comparison: {} (timing verdict above is separate).",
        if comparison.memory_complete() {
            "complete"
        } else {
            "incomplete"
        }
    );
    for issue in comparison.memory_issues() {
        let _ = writeln!(out, "    gap: {issue}");
    }
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
    for (title, deltas, shown) in [
        (
            "Stages",
            comparison.stages(),
            format_nanos as fn(u64) -> String,
        ),
        ("Functions", comparison.functions(), format_nanos),
        ("Tests", comparison.tests(), format_nanos),
        ("Bytes allocated", comparison.allocations(), exact_bytes),
        ("Stored bytes", comparison.storage(), format_bytes),
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
                shown(delta.before().median()),
                shown(delta.after().median()),
                shown(u64::try_from(delta.moved().unsigned_abs()).unwrap_or(u64::MAX)),
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

fn exact_bytes(value: u64) -> String {
    format!("{value} B")
}

type Traffic = BTreeMap<String, Spread>;

/// Compare operation traffic only inside matching producer/state/repeat/mode boundaries.
/// No heap or origin peak is a sum or a cross-process comparison.
fn memory_traffic(before: &RunRecord, after: &RunRecord) -> (Traffic, Traffic, Vec<String>) {
    if before.memory().is_empty() && after.memory().is_empty() {
        return (allocations_of(before), allocations_of(after), vec!["no process inventory; legacy allocation values are approximate and coverage is unknown".into()]);
    }
    let mut issues = Vec::new();
    let earlier = memory_index(before);
    let later = memory_index(after);
    if earlier.len() != before.memory().len() || later.len() != after.memory().len() {
        issues.push("ambiguous duplicate workload keys; allocation comparison withheld".into());
    }
    if earlier.is_empty() || earlier.keys().ne(later.keys()) {
        issues
            .push("process/state/repeat inventories differ; allocation comparison withheld".into());
    }
    if !issues.is_empty() {
        return (Traffic::new(), Traffic::new(), issues);
    }
    let mut left = Traffic::new();
    let mut right = Traffic::new();
    for (key, a) in earlier {
        let b = &later[&key];
        if !a.attribution_complete() || !b.attribution_complete() {
            issues.push(format!("{key}: retained attribution unavailable; allocation traffic is approximate and covers instrumented boundaries only"));
        }
        if a.status != super::MemoryStatus::Available
            || b.status != super::MemoryStatus::Available
            || !a.issues.is_empty()
            || !b.issues.is_empty()
        {
            issues.push(format!(
                "{key}: unavailable or lossy allocation evidence; traffic comparison withheld"
            ));
            continue;
        }
        let (Some(am), Some(bm)) = (&a.capture.manifest, &b.capture.manifest) else {
            continue;
        };
        if am.hotpath_version != bm.hotpath_version
            || am.features != bm.features
            || am.accounting != bm.accounting
            || am.traffic_metric != bm.traffic_metric
            || am.identity.boundary != bm.identity.boundary
            || am.completion != bm.completion
            || am.boundaries != bm.boundaries
            || am.overrides != bm.overrides
            || am.traffic_metric != "bytes"
        {
            issues.push(format!("{key}: incompatible producer, accounting, metric or interval boundary; traffic comparison withheld"));
            continue;
        }
        let rows = |item: &super::MemoryCapture| -> Option<Traffic> {
            let rows = item.functions.as_ref()?["data"].as_array()?;
            let values: Traffic = rows
                .iter()
                .filter_map(|row| {
                    Some((
                        format!("{key}::{}", row["name"].as_str()?),
                        Spread::single(super::parse_bytes(row["total"].as_str()?)?),
                    ))
                })
                .collect();
            (values.len() == rows.len()).then_some(values)
        };
        let (Some(a), Some(b)) = (rows(a), rows(b)) else {
            issues.push(format!(
                "{key}: duplicate operation names or unavailable byte rows; comparison withheld"
            ));
            continue;
        };
        if a.keys().ne(b.keys()) {
            issues.push(format!(
                "{key}: operation inventories differ; comparison withheld"
            ));
            continue;
        }
        left.extend(a);
        right.extend(b);
    }
    (left, right, issues)
}

fn memory_index(record: &RunRecord) -> BTreeMap<String, &super::MemoryCapture> {
    record
        .memory()
        .iter()
        .map(|item| (item.key(), item))
        .collect()
}
