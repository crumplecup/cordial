//! Test timing: reading nextest's events, folding repeats, and ranking.

use cordial::perf::{PerfEnvironment, PerfIdentityBuilder, PerfSubject, RunRecord};
use cordial::perf::{TestRun, measure_from, parse_events, render_timing, summarise};
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;

const EVENTS: &str = concat!(
    "{\"type\":\"suite\",\"event\":\"started\",\"test_count\":4}\n",
    "{\"type\":\"test\",\"event\":\"started\",\"name\":\"cordial::alpha$fast\"}\n",
    "{\"type\":\"test\",\"event\":\"ok\",\"name\":\"cordial::alpha$fast\",\"exec_time\":0.25}\n",
    "{\"type\":\"test\",\"event\":\"ok\",\"name\":\"cordial::alpha$slow\",\"exec_time\":5.5}\n",
    "{\"type\":\"test\",\"event\":\"failed\",\"name\":\"cordial::beta$broken\",\"exec_time\":1.0}\n",
    "{\"type\":\"test\",\"event\":\"ignored\",\"name\":\"cordial::beta$skipped\"}\n",
    "not json output from the build\n",
    "{\"type\":\"suite\",\"event\":\"ok\",\"passed\":2,\"failed\":1}\n",
);

#[test]
fn nextest_events_give_one_run_per_test_that_ran() -> Result<()> {
    cordial::init_tracing();
    let runs = parse_events(EVENTS).into_diagnostic()?;
    let seen: Vec<_> = runs
        .iter()
        .map(|run| {
            (
                run.binary().as_str(),
                run.name().as_str(),
                run.nanos(),
                run.passed(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("cordial::alpha", "fast", 250_000_000, true),
            ("cordial::alpha", "slow", 5_500_000_000, true),
            ("cordial::beta", "broken", 1_000_000_000, false),
        ]
    );
    Ok(())
}

#[test]
fn repeats_fold_into_a_median_per_test_and_a_total_per_repeat() {
    cordial::init_tracing();
    let run = |nanos: u64, passed: bool| {
        vec![
            TestRun::new("b".into(), "one".into(), nanos, true),
            TestRun::new("b".into(), "two".into(), 10, passed),
        ]
    };
    let (tests, totals, failures) = summarise(&[run(100, true), run(300, false), run(200, true)]);
    assert_eq!(totals, [110, 310, 210]);
    assert_eq!(failures, 1, "a test that failed in any repeat counts once");
    let one = tests.iter().find(|test| test.name() == "one");
    assert_eq!(one.map(|test| test.wall().median()), Some(200));
    assert_eq!(one.map(|test| test.wall().count()), Some(3));
}

#[test]
fn the_ranking_lists_binaries_and_the_slowest_tests_first() -> Result<()> {
    cordial::init_tracing();
    let (tests, totals, failures) = summarise(&[parse_events(EVENTS).into_diagnostic()?]);
    let identity = PerfIdentityBuilder::default()
        .sha("abcdef0123")
        .dirty(false)
        .recorded_at(1u64)
        .build()
        .into_diagnostic()?;
    let record = RunRecord::new(
        identity,
        PerfEnvironment::capture(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))),
        PerfSubject::tests("sample"),
        None,
        Vec::new(),
        Vec::new(),
        BTreeMap::from([
            ("repeats".to_owned(), totals.len() as u64),
            ("failed".to_owned(), failures),
        ]),
        tests,
    );
    let text = render_timing(&record, 2);
    let slow = text
        .find("alpha::slow")
        .ok_or_else(|| miette::miette!("{text}"))?;
    let broken = text
        .find("beta::broken")
        .ok_or_else(|| miette::miette!("{text}"))?;
    assert!(slow < broken, "slowest first:\n{text}");
    let fast = text.find("alpha::fast");
    assert!(fast.is_none(), "only the top 2 are listed:\n{text}");
    assert!(text.contains("By binary:"), "{text}");
    assert!(
        text.contains("tests:sample") || text.contains("sample"),
        "{text}"
    );
    Ok(())
}

#[test]
fn a_record_can_be_rebuilt_from_the_raw_logs_of_an_interrupted_run() -> Result<()> {
    cordial::init_tracing();
    let dir = tempfile::tempdir().into_diagnostic()?;
    let first = dir.path().join("first.jsonl");
    let second = dir.path().join("second.jsonl");
    std::fs::write(&first, EVENTS).into_diagnostic()?;
    std::fs::write(&second, EVENTS.replace("5.5", "6.5")).into_diagnostic()?;
    let record = measure_from(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
        "rebuilt",
        4,
        &[first, second],
    )
    .into_diagnostic()?;
    assert_eq!(record.counters().get("repeats"), Some(&2));
    assert_eq!(record.counters().get("threads"), Some(&4));
    assert_eq!(record.counters().get("failed"), Some(&1));
    let slow = record.tests().iter().find(|test| test.name() == "slow");
    assert_eq!(slow.map(|test| test.wall().median()), Some(6_000_000_000));
    Ok(())
}
