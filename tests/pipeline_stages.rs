//! Stopping a run after a pipeline stage: the row it logs has the stages up to
//! there and nothing after, the first stage leaves the run cache warm for the
//! next, and an unknown stage is refused.

mod support {
    pub mod mini_workspace;
    pub mod process_project;
}
use miette::{IntoDiagnostic, Result};
use support::process_project::{Project, Row};

fn seconds(row: &Row, column: &str) -> f64 {
    row[column].parse().unwrap_or(f64::NAN)
}

/// Stop after `stage`, returning the logged row and what the program said it did.
fn stop_after(project: &Project, stage: &str) -> Result<(Row, String)> {
    let output = project.run(&["open", "--stop-after", stage])?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(text.contains(&format!("Stopped after {stage}")), "{text}");
    let row = project.last()?;
    assert_eq!(row["stopped_after"], stage);
    Ok((row, text))
}

#[test]
fn a_run_stops_after_the_stage_it_is_asked_to_and_says_what_it_reused() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = Project {
        project: scratch.path().join("mini"),
        home: scratch.path().join("store"),
    };
    support::mini_workspace::write(&project.project)?;

    // Cold: nothing cached, so it analyses, and says so.
    let (source, said) = stop_after(&project, "source_ir")?;
    assert!(said.contains("crate IR mini: not cached"), "{said}");
    assert!(said.contains("database: workspace loaded"), "{said}");
    assert!(
        said.contains("analysis state: not used (none saved"),
        "the first run has nothing to start from: {said}"
    );
    assert!(
        said.contains("analysis state: saved for the next run"),
        "and leaves its database for the next: {said}"
    );
    assert!(
        said.contains("function evidence (workspace):") && said.contains("evaluated for"),
        "the run says how much of the per-function work was new: {said}"
    );
    assert!(seconds(&source, "source_ir_s") > 0.0);
    assert_eq!(
        seconds(&source, "call_graph_s"),
        0.0,
        "nothing runs after the stage"
    );
    assert_eq!(seconds(&source, "enrich_s"), 0.0);

    // What the run said of the stages before the stop is kept, with where it was stopped: a run
    // stopped to measure a stage is the one whose numbers are wanted.
    let store = cordial::StoreLayout::from_root(project.home.join("mini"), "mini".to_owned());
    let metrics = cordial::read_run_metrics(&store);
    assert_eq!(metrics.values().get("run.ended.stopped_after"), Some(&1));
    assert_eq!(
        metrics.notes().get("run.ended.during").map(String::as_str),
        Some("source_ir")
    );
    assert!(
        metrics
            .values()
            .get("definition_maps.chosen.crates")
            .is_some_and(|chosen| *chosen > 0),
        "the load's own numbers are kept: {:?}",
        metrics.values().keys().collect::<Vec<_>>()
    );

    // The stage before left the crate's IR in the run cache; this run says it
    // used it. The path inclusions were not reached last time, so it projects
    // them now, which is why it loads the database again, and says that too.
    let (graph, said) = stop_after(&project, "call_graph")?;
    assert!(
        said.contains("crate IR mini: reused from the run cache"),
        "{said}"
    );
    assert!(
        said.contains("path inclusions: not cached; projecting them"),
        "{said}"
    );
    assert!(said.contains("database: workspace loaded"), "{said}");
    assert!(
        said.contains("analysis state: restored from the last run"),
        "the database the first run saved is the one this run starts from: {said}"
    );
    assert!(
        graph["cache"].contains("reused from the run cache"),
        "{graph:?}"
    );
    assert!(
        said.contains("call graph: not cached; building it"),
        "{said}"
    );
    assert!(seconds(&graph, "call_graph_s") > 0.0);
    assert_eq!(seconds(&graph, "enrich_s"), 0.0);

    // Now both are cached: nothing is loaded.
    let (enrich, said) = stop_after(&project, "enrich")?;
    assert!(
        said.contains("crate IR mini: reused from the run cache"),
        "{said}"
    );
    assert!(
        said.contains("path inclusions: reused from the run cache"),
        "{said}"
    );
    assert!(
        !said.contains("database:"),
        "no database was needed: {said}"
    );
    assert!(
        said.contains("call graph: reused from the run cache"),
        "the graph built last run is not built again: {said}"
    );
    assert!(
        said.contains("cargo doc mini: started ahead of the scan"),
        "rustdoc runs alongside the analysis: {said}"
    );
    assert_eq!(enrich["loaded"], "0");
    assert!(seconds(&enrich, "enrich_s") > 0.0);
    assert_eq!(seconds(&enrich, "assess_s"), 0.0);

    let (assess, _) = stop_after(&project, "assess")?;
    assert!(seconds(&assess, "assess_s") > 0.0);
    assert_eq!(seconds(&assess, "render_s"), 0.0);

    // Without the flag the run completes and logs no stage.
    project.run(&["open"])?;
    let whole = project.last()?;
    assert_eq!(whole["stopped_after"], "");
    assert!(seconds(&whole, "render_s") > 0.0);
    assert_eq!(whole["findings"], project.plain_findings("whole")?);
    Ok(())
}

#[test]
fn an_unknown_stage_is_refused_and_names_the_choices() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = Project {
        project: scratch.path().join("mini"),
        home: scratch.path().join("store"),
    };
    support::mini_workspace::write(&project.project)?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(&project.project)
        .arg("--store-home")
        .arg(&project.home)
        .args(["open", "--stop-after", "nope"])
        .output()
        .into_diagnostic()?;
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(
        text.contains("source_ir") && text.contains("call_graph"),
        "{text}"
    );
    Ok(())
}
