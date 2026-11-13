//! The run log: one row per run, appended by the run itself.

use cordial::host::take_notes;
use cordial::perf::runlog::{COLUMNS, RunFacts, path, record, row};
use cordial::{GateLedger, StoreLayout};
use miette::{IntoDiagnostic, Result};

const LEDGER: &str = r#"{
  "last_run_wall_nanos": 12500000000,
  "last_run_timings": [
    {"stage":"phase","id":"load_and_probe","elapsed_nanos":10000000000,"invocations":1},
    {"stage":"pipeline","id":"source_ir","elapsed_nanos":6000000000,"invocations":1},
    {"stage":"pipeline","id":"call_graph","elapsed_nanos":3000000000,"invocations":1},
    {"stage":"enrich","id":"scope","elapsed_nanos":500000000,"invocations":1},
    {"stage":"enrich","id":"path-index","elapsed_nanos":250000000,"invocations":1},
    {"stage":"assess","id":"tracing-assessor","elapsed_nanos":100000000,"invocations":1},
    {"stage":"phase","id":"render_and_write","elapsed_nanos":2000000000,"invocations":1}
  ]
}"#;

#[test]
fn a_row_has_one_cell_per_column_and_sums_the_stages() -> Result<()> {
    cordial::init_tracing();
    let ledger: GateLedger = serde_json::from_str(LEDGER).into_diagnostic()?;
    let facts = RunFacts::new(ledger.last_run_timings().clone(), 12.5, 7, None);
    let line = row(1_000, "abcd1234", true, &facts, take_notes());
    let cells: Vec<&str> = line.split(',').collect();
    assert_eq!(cells.len(), COLUMNS.len(), "{line}");
    let cell = |name: &str| -> Result<&str> {
        let index = COLUMNS
            .iter()
            .position(|column| *column == name)
            .ok_or_else(|| miette::miette!("no column {name}"))?;
        Ok(cells[index])
    };
    assert_eq!(cell("commit")?, "abcd1234");
    assert_eq!(cell("dirty")?, "1");
    assert_eq!(cell("mode")?, "process");
    assert_eq!(cell("findings")?, "7");
    assert_eq!(cell("wall_s")?, "12.500");
    assert_eq!(cell("load_and_probe_s")?, "10.000");
    assert_eq!(cell("source_ir_s")?, "6.000");
    assert_eq!(cell("call_graph_s")?, "3.000");
    assert_eq!(cell("enrich_s")?, "0.750", "the enrichers are summed");
    assert_eq!(cell("render_s")?, "2.000");
    Ok(())
}

#[test]
fn each_run_appends_a_row_and_the_header_is_written_once() -> Result<()> {
    cordial::init_tracing();
    let home = tempfile::tempdir().into_diagnostic()?;
    let store = StoreLayout::from_root(home.path().join("p"), "p");
    let ledger: GateLedger = serde_json::from_str(LEDGER).into_diagnostic()?;
    let facts = RunFacts::new(ledger.last_run_timings().clone(), 12.5, 4, None);
    for _ in 0..3 {
        record(&store, home.path(), &facts, take_notes());
    }
    let text = std::fs::read_to_string(path(&store)).into_diagnostic()?;
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "{text}");
    assert_eq!(lines[0], COLUMNS.join(","));
    assert!(
        lines[1..]
            .iter()
            .all(|line| line.split(',').count() == COLUMNS.len())
    );
    Ok(())
}
