//! Per-test profiles: reading `hotpath` reports and joining them to timings.

use cordial::perf::{
    PerfEnvironment, PerfIdentityBuilder, PerfSubject, RunRecord, Spread, TestTiming, aggregate,
    attach, parse_duration, parse_report, profiled, read_profiles, render_profile,
};
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;
use std::fs;

const REPORT: &str = r#"{
  "type": "hotpath_report",
  "functions_timing": {
    "total_elapsed_ns": 8000000000,
    "data": [
      {"id": 1, "name": "test", "calls": 1, "avg": "8.00 s", "p95": "8.00 s", "total": "8.00 s"},
      {"id": 2, "name": "cordial::loader::SourceDatabase::load_reporting", "calls": 1,
       "avg": "6.00 s", "p95": "6.00 s", "total": "6.00 s"},
      {"id": 3, "name": "cordial::loader::decode", "calls": 4,
       "avg": "250.00 ms", "p95": "400.00 ms", "total": "1.00 s"},
      {"id": 4, "name": "cordial::loader::projection::selection::project_target", "calls": 1,
       "avg": "1.00 s", "p95": "1.00 s", "total": "1.00 s"}
    ]
  }
}"#;

#[test]
fn durations_read_in_every_unit_hotpath_prints() {
    cordial::init_tracing();
    assert_eq!(parse_duration("14.35 s"), Some(14_350_000_000));
    assert_eq!(parse_duration("10.34 ms"), Some(10_340_000));
    assert_eq!(parse_duration("14.97 µs"), Some(14_970));
    assert_eq!(parse_duration("80 ns"), Some(80));
    assert_eq!(parse_duration("soon"), None);
}

#[test]
fn a_report_gives_its_functions_and_lifetime() -> Result<()> {
    cordial::init_tracing();
    let (wall, functions) = parse_report(REPORT).into_diagnostic()?;
    assert_eq!(wall, 8_000_000_000);
    assert_eq!(functions.len(), 4);
    assert_eq!(functions[2].calls(), 4);
    assert_eq!(functions[2].p95_ns(), 400_000_000);
    Ok(())
}

fn timing_record() -> Result<RunRecord> {
    let wall = Spread::from_samples(&[9_000_000_000]).ok_or_else(|| miette::miette!("spread"))?;
    let test = TestTiming::new(
        "cordial::alpha".to_owned(),
        "slow".to_owned(),
        wall,
        Vec::new(),
        0,
        BTreeMap::new(),
    );
    let identity = PerfIdentityBuilder::default()
        .sha("abc")
        .dirty(false)
        .recorded_at(1u64)
        .build()
        .into_diagnostic()?;
    Ok(RunRecord::new(
        identity,
        PerfEnvironment::capture(std::path::Path::new(".")),
        PerfSubject::tests("set"),
        None,
        Vec::new(),
        Vec::new(),
        BTreeMap::new(),
        vec![test],
    ))
}

#[test]
fn profiles_give_a_test_its_fixed_cost_and_the_run_its_functions() -> Result<()> {
    cordial::init_tracing();
    let directory = tempfile::tempdir().into_diagnostic()?;
    fs::write(directory.path().join("cordial::alpha$slow.json"), REPORT).into_diagnostic()?;
    fs::write(directory.path().join("notes.txt"), "ignored").into_diagnostic()?;
    let profiles = read_profiles(directory.path()).into_diagnostic()?;
    assert_eq!(profiles.len(), 1);
    assert_eq!(aggregate(&profiles)[0].name(), "test");

    let record = profiled(&timing_record()?, "set", &profiles);
    assert_eq!(record.subject().to_string(), "tests:set+profile");
    let test = &record.tests()[0];
    assert_eq!(test.fixed_ns(), 7_000_000_000);
    assert!((test.fixed_share() - 7.0 / 9.0).abs() < 1e-9);
    assert_eq!(test.stages()[0].name(), "test");
    assert_eq!(record.functions().len(), 4);
    assert_eq!(record.counters().get("profiled"), Some(&1));

    let text = render_profile(&record, 5);
    assert!(text.contains("setup 7.00s of 9.00s (77.8%)"), "{text}");
    assert!(text.contains("cordial::alpha::slow"), "{text}");
    Ok(())
}

#[test]
fn a_test_without_a_profile_is_left_as_measured() -> Result<()> {
    cordial::init_tracing();
    let record = timing_record()?;
    let attached = attach(record.tests().clone(), &[]);
    assert_eq!(attached[0].fixed_ns(), 0);
    assert!(attached[0].stages().is_empty());
    Ok(())
}

#[test]
fn a_profile_file_that_is_not_a_report_is_an_error_naming_it() -> Result<()> {
    cordial::init_tracing();
    let directory = tempfile::tempdir().into_diagnostic()?;
    fs::write(directory.path().join("cordial::alpha$slow.json"), "nope").into_diagnostic()?;
    let error = read_profiles(directory.path()).err().ok_or_else(|| miette::miette!("accepted"))?;
    assert!(error.to_string().contains("slow.json"), "{error}");
    Ok(())
}
