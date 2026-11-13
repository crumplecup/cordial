//! What a profile says besides time: allocation, threads, locks, channels and the
//! sizes the program reports, read from a `hotpath` report into a run record,
//! joined across tests, and compared between two runs.

use cordial::perf::{
    PerfEnvironment, PerfIdentityBuilder, PerfSubject, ProfileUsage, RunRecord, RunRecordBuilder,
    Spread, TestTiming, compare, counts_exactly, format_bytes, parse_bytes, profiled,
    read_profiles, render_comparison, render_profile,
};
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;

/// The shapes of a real report with every section fed, as a profile of a small
/// test wrote them (allocation counting on, a lock and a channel used, sizes
/// reported through `val!`).
const REPORT: &str = r#"{
  "type": "hotpath_report",
  "functions_timing": {"total_elapsed_ns": 2000000000, "data": [
    {"id": 1, "name": "test", "calls": 1, "avg": "2.00 s", "p95": "2.00 s", "total": "2.00 s"}]},
  "functions_alloc": {"profiling_mode": "alloc-bytes", "data": [
    {"id": 14, "name": "cordial::loader::database::context_crates::functions_of", "calls": 3,
     "avg": "672.8 MB", "p95": "954.0 MB", "total": "2.0 GB"},
    {"id": 15, "name": "cordial::loader::database::session::workspace::load_database", "calls": 1,
     "avg": "130.6 MB", "p95": "130.6 MB", "total": "130.6 MB"}]},
  "threads": {"data": [
    {"os_tid": 1, "name": "an_edit_reaches", "cpu_percent_max": "103.7%",
     "alloc_bytes": "2.1 GB", "dealloc_bytes": "1.9 GB", "mem_diff": "163.0 MB"},
    {"os_tid": 2, "name": "cordial-prime-0", "cpu_percent_max": "98.0%"}]},
  "mutexes": {"data": [
    {"id": 1, "label": "run-events", "count": 3, "wait_avg": "890 ns",
     "wait_percentiles": {"p95": "1.11 µs"}}]},
  "channels": {"data": [
    {"id": 1, "label": "profile-done", "instances": 1, "sent_count": 4, "received_count": 4}]},
  "debug": {"entries": [
    {"entry_type": "val", "expression": "storage.context.+kani.files", "last_value": "2"},
    {"entry_type": "val", "expression": "storage.context.+kani.bytes", "last_value": "221"},
    {"entry_type": "val", "expression": "storage.store.cache.bytes", "last_value": "13631488"},
    {"entry_type": "dbg", "expression": "ignored", "last_value": "7"}]}
}"#;

fn usage() -> Result<ProfileUsage> {
    let report: serde_json::Value = serde_json::from_str(REPORT).into_diagnostic()?;
    Ok(ProfileUsage::from_report(&report))
}

#[test]
fn byte_counts_read_in_every_unit_hotpath_prints() {
    cordial::init_tracing();
    assert_eq!(parse_bytes("221 B"), Some(221));
    assert_eq!(parse_bytes("1.5 KB"), Some(1536));
    assert_eq!(parse_bytes("163.0 MB"), Some(170_917_888));
    assert_eq!(parse_bytes("2.0 GB"), Some(2_147_483_648));
    assert_eq!(parse_bytes("soon"), None);
    assert_eq!(format_bytes(221), "221 B");
    assert_eq!(format_bytes(170_917_888), "163.0 MB");
    assert!(counts_exactly("storage.context.+kani.files"));
    assert!(counts_exactly("storage.store.cache.files"));
    assert!(
        !counts_exactly("storage.salsa.query.direct_facts.memos"),
        "rust-analyzer's own counts move with its version and trend"
    );
    assert!(!counts_exactly("storage.store.cache.bytes"));
}

#[test]
fn a_report_gives_what_it_says_besides_time() -> Result<()> {
    cordial::init_tracing();
    let usage = usage()?;
    assert_eq!(usage.allocations().len(), 2);
    assert_eq!(usage.allocations()[0].calls(), 3);
    assert_eq!(usage.allocations()[0].total_bytes(), 2_147_483_648);
    assert_eq!(usage.threads().len(), 2);
    assert_eq!(usage.threads()[0].cpu_peak_tenths(), 1037);
    assert!(usage.threads()[0].alloc_bytes() > usage.threads()[0].dealloc_bytes());
    assert_eq!(
        usage.threads()[1].alloc_bytes(),
        0,
        "a thread with no bytes reads as none"
    );
    assert_eq!(usage.locks()[0].label(), "run-events");
    assert_eq!(usage.locks()[0].wait_p95_ns(), 1110);
    assert_eq!(usage.channels()[0].sent(), 4);
    assert_eq!(usage.values().len(), 3, "only `val` entries are values");
    assert_eq!(usage.values()["storage.context.+kani.files"], 2);
    // Only what counts things exactly gates.
    let exact = usage.exact_counters();
    assert_eq!(exact.len(), 1);
    assert_eq!(exact["value.storage.context.+kani.files"], 2);
    Ok(())
}

#[test]
fn a_report_without_those_sections_reads_as_empty() -> Result<()> {
    cordial::init_tracing();
    let report: serde_json::Value =
        serde_json::from_str(r#"{"functions_timing": {"data": []}}"#).into_diagnostic()?;
    assert_eq!(ProfileUsage::from_report(&report), ProfileUsage::default());
    Ok(())
}

#[test]
fn several_profiles_add_up_the_way_a_run_does() -> Result<()> {
    cordial::init_tracing();
    let one = usage()?;
    let merged = ProfileUsage::merge([&one, &one]);
    assert_eq!(merged.allocations()[0].calls(), 6);
    assert_eq!(merged.allocations()[0].total_bytes(), 2 * 2_147_483_648);
    assert_eq!(
        merged.threads()[0].cpu_peak_tenths(),
        1037,
        "a thread keeps its highest use"
    );
    assert_eq!(merged.locks()[0].count(), 6);
    assert_eq!(merged.locks()[0].wait_avg_ns(), 890);
    assert_eq!(merged.channels()[0].sent(), 8);
    assert_eq!(
        merged.values()["storage.context.+kani.files"],
        2,
        "a value keeps its largest"
    );
    Ok(())
}

fn record(report: &str, set: &str) -> Result<RunRecord> {
    let directory = tempfile::tempdir().into_diagnostic()?;
    std::fs::write(directory.path().join("cordial::alpha$one.json"), report).into_diagnostic()?;
    let profiles = read_profiles(directory.path()).into_diagnostic()?;
    let wall = Spread::from_samples(&[2_000_000_000]).ok_or_else(|| miette::miette!("spread"))?;
    let test = TestTiming::new(
        "cordial::alpha".to_owned(),
        "one".to_owned(),
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
    let timing = RunRecordBuilder::default()
        .identity(identity)
        .environment(PerfEnvironment::capture(std::path::Path::new(".")))
        .subject(PerfSubject::tests(set))
        .tests(vec![test])
        .build()
        .into_diagnostic()?;
    profiled(&timing, set, &profiles).into_diagnostic()
}

#[test]
fn a_profiled_record_carries_the_sections_and_gates_on_the_exact_counts() -> Result<()> {
    cordial::init_tracing();
    let record = record(REPORT, "set")?;
    assert_eq!(record.allocations().len(), 2);
    assert_eq!(record.values()["storage.store.cache.bytes"], 13_631_488);
    assert_eq!(record.counters()["value.storage.context.+kani.files"], 2);
    assert!(
        !record.counters().keys().any(|key| key.ends_with(".bytes")),
        "a size trends and does not gate"
    );
    let text = render_profile(&record, 5);
    assert!(text.contains("Functions by bytes allocated"), "{text}");
    assert!(
        text.contains("Storage and other values (largest 3 of 3)"),
        "{text}"
    );
    assert!(
        text.contains("12.9 MB  storage.store.cache.bytes") || text.contains("13.0 MB"),
        "{text}"
    );
    assert!(text.contains("run-events"), "{text}");
    assert!(text.contains("Threads by bytes allocated"), "{text}");
    Ok(())
}

#[test]
fn a_record_written_before_these_sections_still_reads() -> Result<()> {
    cordial::init_tracing();
    let mut value = serde_json::to_value(record(REPORT, "set")?).into_diagnostic()?;
    for key in ["allocations", "threads", "locks", "channels", "values"] {
        value
            .as_object_mut()
            .ok_or_else(|| miette::miette!("a record is an object"))?
            .remove(key);
    }
    let old: RunRecord = serde_json::from_value(value).into_diagnostic()?;
    assert!(old.allocations().is_empty() && old.values().is_empty());
    Ok(())
}

#[test]
fn a_comparison_gates_on_counts_and_shows_the_bytes_that_moved() -> Result<()> {
    cordial::init_tracing();
    let before = record(REPORT, "set")?;
    // More files copied, more bytes stored, more allocated.
    let grown = REPORT
        .replace(
            "storage.context.+kani.files\", \"last_value\": \"2\"",
            "storage.context.+kani.files\", \"last_value\": \"9\"",
        )
        .replace(
            "\"last_value\": \"13631488\"",
            "\"last_value\": \"27262976\"",
        )
        .replace("\"total\": \"2.0 GB\"", "\"total\": \"4.0 GB\"");
    let after = record(&grown, "set")?;

    let comparison = compare(&before, &after, 0.10);
    let gated: Vec<_> = comparison
        .counters()
        .iter()
        .filter(|counter| counter.gated())
        .map(|counter| counter.name().clone())
        .collect();
    assert_eq!(gated, vec!["value.storage.context.+kani.files".to_owned()]);
    assert_eq!(comparison.verdict().to_string(), "regressed");
    assert_eq!(comparison.storage().len(), 2, "both byte values are paired");
    let text = render_comparison(&comparison, 5);
    assert!(text.contains("Stored bytes that moved"), "{text}");
    assert!(text.contains("Bytes allocated that moved"), "{text}");
    assert!(text.contains("storage.store.cache.bytes"), "{text}");
    Ok(())
}
