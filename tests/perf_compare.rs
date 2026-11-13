//! Comparing records: verdicts, exact counters, baselines and changed inputs.

use cordial::StoreLayout;
use cordial::perf::{
    ChangeKind, PerfEnvironment, PerfIdentityBuilder, PerfState, PerfStore, PerfSubject, RunRecord,
    RunRecordBuilder, Spread, StageTiming, Verdict, compare, judge, render_comparison,
};
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;

fn spread(median: u64, mad: u64) -> Spread {
    // Three samples with the requested median and deviation.
    Spread::from_samples(&[median - mad, median, median + mad]).unwrap_or(Spread::single(median))
}

fn environment() -> PerfEnvironment {
    PerfEnvironment::capture(std::path::Path::new("."))
}

fn record(
    at: u64,
    time: Spread,
    counters: &[(&str, u64)],
    environment: PerfEnvironment,
) -> Result<RunRecord> {
    let identity = PerfIdentityBuilder::default()
        .sha("abcdef0123")
        .dirty(false)
        .recorded_at(at)
        .build()
        .into_diagnostic()?;
    RunRecordBuilder::default()
        .identity(identity)
        .environment(environment)
        .subject(PerfSubject::workflow("cordial"))
        .state(Some(PerfState::Cold))
        .stages(vec![StageTiming::new(
            "phase/load_and_probe".to_owned(),
            time,
        )])
        .counters(
            counters
                .iter()
                .map(|(name, value)| ((*name).to_owned(), *value))
                .collect::<BTreeMap<_, _>>(),
        )
        .build()
        .into_diagnostic()
}

#[test]
fn a_move_counts_only_beyond_tolerance_and_noise() {
    cordial::init_tracing();
    let base = spread(10_000, 100);
    assert_eq!(
        judge(base, spread(10_500, 100), 0.10),
        Verdict::Unchanged,
        "5% is inside 10%"
    );
    assert_eq!(judge(base, spread(13_000, 100), 0.10), Verdict::Regressed);
    assert_eq!(judge(base, spread(7_000, 100), 0.10), Verdict::Improved);
    let noisy = spread(10_000, 4_000);
    assert_eq!(
        judge(noisy, spread(12_000, 4_000), 0.10),
        Verdict::Unchanged,
        "inside the noise"
    );
}

#[test]
fn each_verdict_follows_from_what_moved() -> Result<()> {
    cordial::init_tracing();
    let base = record(
        1,
        spread(10_000, 50),
        &[("hooks.enrich", 30)],
        environment(),
    )?;
    let same = record(
        2,
        spread(10_020, 50),
        &[("hooks.enrich", 30)],
        environment(),
    )?;
    let slower = record(
        3,
        spread(15_000, 50),
        &[("hooks.enrich", 30)],
        environment(),
    )?;
    let faster = record(4, spread(5_000, 50), &[("hooks.enrich", 30)], environment())?;
    let more_work = record(
        5,
        spread(10_000, 50),
        &[("hooks.enrich", 31)],
        environment(),
    )?;
    let less_work = record(
        6,
        spread(10_000, 50),
        &[("hooks.enrich", 29)],
        environment(),
    )?;
    assert_eq!(compare(&base, &same, 0.10).verdict(), Verdict::Unchanged);
    assert_eq!(compare(&base, &slower, 0.10).verdict(), Verdict::Regressed);
    assert_eq!(compare(&base, &faster, 0.10).verdict(), Verdict::Improved);
    assert_eq!(
        compare(&base, &more_work, 0.10).verdict(),
        Verdict::Regressed,
        "a counter that grew regresses at identical time"
    );
    assert_eq!(
        compare(&base, &less_work, 0.10).verdict(),
        Verdict::Improved
    );
    Ok(())
}

#[test]
fn counters_that_describe_the_run_do_not_gate() -> Result<()> {
    cordial::init_tracing();
    let base = record(1, spread(10_000, 50), &[("threads", 1)], environment())?;
    let more = record(2, spread(10_000, 50), &[("threads", 8)], environment())?;
    let comparison = compare(&base, &more, 0.10);
    assert_eq!(comparison.verdict(), Verdict::Unchanged);
    assert_eq!(comparison.counters().len(), 1, "still shown");
    Ok(())
}

#[test]
fn a_dependency_update_is_compared_and_attributed() -> Result<()> {
    cordial::init_tracing();
    let before = environment();
    let mut after = serde_json::to_value(&before).into_diagnostic()?;
    after["dependencies"]["ra_ap_hir"] = serde_json::json!("9.9.9");
    let after: PerfEnvironment = serde_json::from_value(after).into_diagnostic()?;
    let old = record(1, spread(10_000, 50), &[], before.clone())?;
    let new = record(2, spread(15_000, 50), &[], after)?;
    let comparison = compare(&old, &new, 0.10);
    assert_eq!(comparison.verdict(), Verdict::Regressed, "not withheld");
    assert_eq!(comparison.changed_inputs().len(), 1);
    assert_eq!(
        comparison.changed_inputs()[0].kind(),
        ChangeKind::Dependency
    );
    assert!(!comparison.machine_changed());
    let text = render_comparison(&comparison, 10);
    assert!(text.contains("dependency"), "{text}");
    assert!(text.contains("ra_ap_hir"), "{text}");
    assert!(text.contains("9.9.9"), "{text}");
    assert!(!text.contains("Caution"), "{text}");
    Ok(())
}

#[test]
fn a_different_machine_is_compared_with_a_caution() -> Result<()> {
    cordial::init_tracing();
    let before = environment();
    let mut after = serde_json::to_value(&before).into_diagnostic()?;
    after["cores"] = serde_json::json!(before.cores() + 4);
    let after: PerfEnvironment = serde_json::from_value(after).into_diagnostic()?;
    let old = record(1, spread(10_000, 50), &[("hooks.enrich", 3)], before)?;
    let new = record(2, spread(5_000, 50), &[("hooks.enrich", 3)], after)?;
    let comparison = compare(&old, &new, 0.10);
    assert!(comparison.machine_changed());
    assert_eq!(comparison.verdict(), Verdict::Improved, "still reported");
    assert!(render_comparison(&comparison, 10).contains("different hardware"));
    Ok(())
}

#[test]
fn baselines_are_marked_explicitly_and_records_found_by_prefix() -> Result<()> {
    cordial::init_tracing();
    let home = tempfile::tempdir().into_diagnostic()?;
    let layout = StoreLayout::from_root(home.path().join("p"), "p");
    let store = PerfStore::new(&layout);
    let first = record(1_000, spread(10_000, 50), &[], environment())?;
    let second = record(2_000, spread(12_000, 50), &[], environment())?;
    store.save(&first).into_diagnostic()?;
    store.save(&second).into_diagnostic()?;

    assert!(
        store.baselines().into_diagnostic()?.is_empty(),
        "saving marks nothing"
    );
    store.set_baseline(&first).into_diagnostic()?;
    let key = PerfStore::baseline_key(&first);
    assert_eq!(key, "workflow:cordial cold");
    assert_eq!(
        store.baselines().into_diagnostic()?.get(&key),
        Some(&first.id())
    );

    assert_eq!(store.find("0000001000").into_diagnostic()?.id(), first.id());
    assert!(
        store.find("00000").is_err(),
        "an ambiguous prefix is refused"
    );
    assert!(store.find("nope").is_err());
    let latest = store
        .latest("workflow:cordial", Some(PerfState::Cold))
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no latest"))?;
    assert_eq!(latest.id(), second.id());
    Ok(())
}
