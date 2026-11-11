//! The ledger's counts and the timing passes the worklist records.

use miette::IntoDiagnostic;
use std::fs;

#[path = "support/cli_worklist_project.rs"]
mod cli_worklist_project;

use cli_worklist_project::{fixture, run};

#[test]
fn quality_records_last_run_counts_and_hook_costs_in_the_ledger() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let gate = run(project.path(), home.path(), &["quality"])?;
    assert!(
        gate.status.success(),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let ledger = cordial::GateLedger::read(&store).into_diagnostic()?;
    let panics = ledger
        .entries()
        .get("panics")
        .ok_or_else(|| miette::miette!("panics missing from ledger"))?;
    assert!(*panics.last_run_unix_secs() > 0);
    assert!(*panics.open_count() > 0);
    assert!(*panics.wall_time_nanos() > 0);
    assert!(*ledger.last_run_wall_nanos() > 0);
    assert!(
        ledger
            .last_run_timings()
            .iter()
            .any(|timing| timing.stage() == "phase" && timing.id() == "load_and_probe")
    );
    assert!(
        ledger
            .last_run_timings()
            .iter()
            .any(|timing| timing.stage() == "pipeline" && timing.id() == "artifact_write")
    );
    Ok(())
}

#[test]
fn timings_measure_two_passes_without_replacing_quality_outputs() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let gate = run(project.path(), home.path(), &["quality"])?;
    assert!(gate.status.success());
    let report_path = store.findings_dir().join("quality-report.md");
    let previous_report = fs::read(&report_path).into_diagnostic()?;
    let previous_ledger = cordial::GateLedger::read(&store).into_diagnostic()?;

    let measured = run(
        project.path(),
        home.path(),
        &["timings", "--etiquette", "panics"],
    )?;
    assert!(
        measured.status.success(),
        "{}",
        String::from_utf8_lossy(&measured.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(store.root().join("timings.json")).into_diagnostic()?)
            .into_diagnostic()?;
    assert_eq!(json["version"], 1);
    assert_eq!(json["entries"][0]["id"], "panics");
    assert!(
        json["entries"][0]["cold"]["wall_time_nanos"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );
    assert!(
        json["entries"][0]["warm"]["wall_time_nanos"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );
    let markdown = fs::read_to_string(store.root().join("timings.md")).into_diagnostic()?;
    assert!(markdown.contains("## Pipeline phases"));
    assert_eq!(fs::read(report_path).into_diagnostic()?, previous_report);
    assert_eq!(
        cordial::GateLedger::read(&store).into_diagnostic()?,
        previous_ledger
    );
    Ok(())
}

#[test]
fn timings_marginal_compares_exact_scoped_sets() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let measured = run(
        project.path(),
        home.path(),
        &[
            "timings",
            "--marginal",
            "--scope",
            "panics,allows",
            "--etiquette",
            "panics",
        ],
    )?;
    assert!(
        measured.status.success(),
        "{}",
        String::from_utf8_lossy(&measured.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(store.root().join("timings.json")).into_diagnostic()?)
            .into_diagnostic()?;
    assert_eq!(json["version"], 2);
    assert_eq!(json["entries"][0]["id"], "panics");
    assert_eq!(
        json["entries"][0]["marginal"]["scope"],
        serde_json::json!(["allows", "panics"])
    );
    for pass in ["cold", "warm"] {
        let pair = &json["entries"][0]["marginal"][pass];
        assert_eq!(
            pair["full"]["included"],
            serde_json::json!(["allows", "panics"])
        );
        assert_eq!(pair["without"]["included"], serde_json::json!(["allows"]));
        assert!(pair["full"]["wall_time_nanos"].as_u64().unwrap_or(0) > 0);
        assert!(pair["without"]["wall_time_nanos"].as_u64().unwrap_or(0) > 0);
        let full = pair["full"]["wall_time_nanos"]
            .as_i64()
            .ok_or_else(|| miette::miette!("full wall time is not an integer"))?;
        let without = pair["without"]["wall_time_nanos"]
            .as_i64()
            .ok_or_else(|| miette::miette!("omitted wall time is not an integer"))?;
        let marginal = pair["wall_delta_nanos"]
            .as_i64()
            .ok_or_else(|| miette::miette!("marginal wall time is not an integer"))?;
        assert_eq!(marginal, full - without);
        let standalone = json["entries"][0][pass]["wall_time_nanos"]
            .as_i64()
            .ok_or_else(|| miette::miette!("standalone wall time is not an integer"))?;
        let overlap_key = format!("{pass}_overlap_nanos");
        assert_eq!(
            json["entries"][0]["marginal"][overlap_key.as_str()].as_i64(),
            Some(standalone - marginal)
        );
        assert!(
            pair["stage_deltas"]
                .as_array()
                .is_some_and(|deltas| deltas.iter().any(|delta| delta["stage"] == "phase"))
        );
    }
    let markdown = fs::read_to_string(store.root().join("timings.md")).into_diagnostic()?;
    assert!(markdown.contains("## Marginal cost (full minus without)"));
    assert!(markdown.contains("### Marginal pipeline phases"));
    Ok(())
}

#[test]
fn timings_marginal_can_omit_the_only_selected_etiquette() -> miette::Result<()> {
    cordial::init_tracing();
    let (project, home, store) = fixture()?;
    let measured = run(
        project.path(),
        home.path(),
        &[
            "timings",
            "--marginal",
            "--scope",
            "panics",
            "--etiquette",
            "panics",
        ],
    )?;
    assert!(
        measured.status.success(),
        "{}",
        String::from_utf8_lossy(&measured.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(store.root().join("timings.json")).into_diagnostic()?)
            .into_diagnostic()?;
    let without = &json["entries"][0]["marginal"]["warm"]["without"];
    assert_eq!(without["included"], serde_json::json!([]));
    assert_eq!(without["hooks"], serde_json::json!([]));
    Ok(())
}
