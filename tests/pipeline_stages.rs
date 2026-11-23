//! Stopping a run after a pipeline stage: the row it logs has the stages up to
//! there and nothing after, the first stage leaves the run cache warm for the
//! next, and an unknown stage is refused.

mod support {
    pub mod mini_workspace;
    pub mod process_project;
}
use cordial::{FOOTPRINT_VARIABLE, StoreLayout, read_run_metrics};
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
            .get("definition_maps.chosen_first.crates")
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

fn project_in(scratch: &std::path::Path, name: &str) -> Result<Project> {
    let project = Project {
        project: scratch.join(name).join("mini"),
        home: scratch.join(name).join("store"),
    };
    support::mini_workspace::write(&project.project)?;
    Ok(project)
}

fn metrics_of(project: &Project) -> cordial::RunMetrics {
    read_run_metrics(&StoreLayout::from_root(
        project.home.join("mini"),
        "mini".to_owned(),
    ))
}

#[test]
fn a_run_asked_for_the_footprint_says_what_the_database_holds_as_it_grows() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = project_in(scratch.path(), "asked")?;
    project.run_with(
        &["open", "--stop-after", "source_ir"],
        FOOTPRINT_VARIABLE,
        "1",
    )?;
    let metrics = metrics_of(&project);
    let values = metrics.values();
    let value = |name: &str| values.get(name).copied().unwrap_or(0);

    let loaded = value("footprint.workspace_loaded_query_total.metadata_bytes")
        + value("footprint.workspace_loaded_query_total.fields_bytes");
    let primed = value("footprint.function_evidence_primed_query_total.metadata_bytes")
        + value("footprint.function_evidence_primed_query_total.fields_bytes");
    assert!(
        primed > loaded,
        "the queries hold more once the evidence is primed: {loaded} then {primed}"
    );
    assert!(
        value("footprint.function_evidence_primed_query_total.memos")
            > value("footprint.workspace_loaded_query_total.memos")
    );
    assert!(
        value("footprint.function_evidence_primed_query_total.micros") > 0,
        "the walk is timed"
    );
    assert!(
        value("footprint.function_evidence_primed_struct_total.fields_bytes") > 0,
        "and the structs are counted"
    );
    // The kinds that hold the most are named: the evidence is a persisted query of the tool's.
    assert!(
        values.keys().any(
            |name| name.starts_with("footprint.function_evidence_primed_query_")
                && !name.contains("_total.")
        ),
        "{:?}",
        values
            .keys()
            .filter(|name| name.starts_with("footprint."))
            .take(8)
            .collect::<Vec<_>>()
    );
    assert!(
        values
            .keys()
            .any(|name| name.starts_with("footprint.definition_maps_built_call_first_")),
        "the definition maps' point is there too"
    );
    Ok(())
}

#[test]
fn a_run_not_asked_does_not_walk_the_database() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = project_in(scratch.path(), "unasked")?;
    project.run(&["open", "--stop-after", "source_ir"])?;
    let metrics = metrics_of(&project);
    assert!(
        !metrics
            .values()
            .keys()
            .any(|name| name.starts_with("footprint.")),
        "no footprint was asked for"
    );
    Ok(())
}

#[test]
fn the_priming_says_what_each_chunk_took_and_what_it_took_to_take_it() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = project_in(scratch.path(), "chunks")?;
    project.run(&["open", "--stop-after", "source_ir"])?;
    let metrics = metrics_of(&project);
    let values = metrics.values();
    assert!(
        values.get("prime.workspace.done").copied().unwrap_or(0) > 0,
        "{:?}",
        values
            .keys()
            .filter(|name| name.starts_with("prime."))
            .collect::<Vec<_>>()
    );
    assert!(
        values.get("prime.workspace.micros").copied().unwrap_or(0) > 0,
        "the chunk is timed"
    );
    assert!(values.contains_key("prime.workspace.grown_kib"));
    assert!(values.contains_key("prime.workspace.resident_after_kib"));
    Ok(())
}

/// Asked for, the first project of a store makes the base state (the sysroot, built and saved once), and
/// every project with no state of its own after it starts from that and does not make it again; not
/// asked for, a project starts cold and nothing is made.
#[test]
fn a_store_makes_its_base_state_once_and_projects_with_no_state_start_from_it() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let home = scratch.path().join("store");
    let project = |name: &str| -> Result<Project> {
        let project = Project {
            project: scratch.path().join(name),
            home: home.clone(),
        };
        support::mini_workspace::write(&project.project)?;
        Ok(project)
    };
    let said = |output: std::process::Output| String::from_utf8_lossy(&output.stdout).into_owned();
    let stop = ["open", "--stop-after", "source_ir"];

    // Not asked for: cold, and nothing is made.
    let cold = said(project("off")?.run(&stop)?);
    assert!(!cold.contains("base state"), "{cold}");
    assert!(cold.contains("not used (none saved"), "{cold}");
    assert!(
        !home.join("sysroot").exists(),
        "no base was made: the shared cache was not even made"
    );

    // What an earlier build of the tool left in the store is not kept when this build makes its own.
    let old = home.join("sysroot/cache/base-state/0123456789abcdef");
    std::fs::create_dir_all(&old).into_diagnostic()?;
    std::fs::write(old.join("key"), "an older build").into_diagnostic()?;

    // The first project that wants one makes it.
    let first = said(project("first")?.run_with(&stop, cordial::BASE_VARIABLE, "on")?);
    assert!(
        first.contains("base state: none yet for this build; making it"),
        "{first}"
    );
    assert!(first.contains("base state: made in"), "{first}");
    assert!(first.contains("started from the base state"), "{first}");
    assert!(!old.exists(), "the base of another build was removed");
    let status =
        said(project("first")?.run_with(&["base", "status"], cordial::BASE_VARIABLE, "on")?);
    assert!(status.contains("base state: "), "{status}");
    assert!(!status.contains("none for this build"), "{status}");

    // A second project in the same store starts from it, and makes nothing.
    let second = said(project("second")?.run_with(&stop, cordial::BASE_VARIABLE, "on")?);
    assert!(!second.contains("making it"), "{second}");
    assert!(second.contains("started from the base state"), "{second}");

    // And a project that has a state of its own of this build uses that.
    let second_project = project("second")?;
    let lib = second_project.project.join("src/lib.rs");
    let text = std::fs::read_to_string(&lib).into_diagnostic()?;
    std::fs::write(
        &lib,
        format!("{text}\n// an edit, so that the run loads the database\n"),
    )
    .into_diagnostic()?;
    let again = said(second_project.run_with(&stop, cordial::BASE_VARIABLE, "on")?);
    assert!(again.contains("restored from the last run"), "{again}");
    assert!(!again.contains("started from the base state"), "{again}");
    Ok(())
}
