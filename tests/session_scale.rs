//! How an edit costs as a project grows: a generated crate of many modules, run
//! through a session cold and then after edits, with each step's time by segment.
//! Size it with CORDIAL_SCALE_FILES and CORDIAL_SCALE_FUNCTIONS.

mod support {
    pub mod scaled_workspace;
    pub mod session_project;
}
use miette::{IntoDiagnostic, Result};
use support::session_project::{Project, Row, append};

fn size(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn seconds(row: &Row, column: &str) -> f64 {
    row[column].parse().unwrap_or(f64::NAN)
}

/// One step's line in the table.
fn report(step: &str, row: &Row) -> String {
    format!(
        "{step:<18} wall {:>7.2}s  source_ir {:>7.2}s  call_graph {:>7.2}s  enrich {:>6.2}s  \
         loaded {} kept {} reloaded {} files {} cache {}",
        seconds(row, "wall_s"),
        seconds(row, "source_ir_s"),
        seconds(row, "call_graph_s"),
        seconds(row, "enrich_s"),
        row["loaded"],
        row["kept"],
        row["reloaded"],
        row["files_changed"],
        row["cache_hits"],
    )
}

#[test]
fn an_edit_in_a_session_costs_a_fraction_of_the_cold_load_as_the_project_grows() -> Result<()> {
    cordial::init_tracing();
    let files = size("CORDIAL_SCALE_FILES", 30);
    let per_file = size("CORDIAL_SCALE_FUNCTIONS", 10);
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = Project {
        project: scratch.path().join("mini"),
        home: scratch.path().join("store"),
    };
    support::scaled_workspace::write(&project.project, files, per_file)?;
    let source = |module: usize| project.project.join(format!("src/m{module}.rs"));

    project.run(&["session", "start"], true)?;
    project.run(&["open"], true)?;
    let cold = project.last()?;
    let mut table = vec![report("cold open", &cold)];
    assert_eq!(cold["loaded"], "1");

    let unchanged = project.submit()?;
    table.push(report("unchanged", &unchanged));
    assert_eq!(unchanged["cache_hits"], "1", "{unchanged:?}");

    let middle = files / 2;
    append(&source(middle), "\n// a comment\n")?;
    let commented = project.submit()?;
    table.push(report("comment", &commented));

    let text = std::fs::read_to_string(source(middle)).into_diagnostic()?;
    std::fs::write(source(middle), text.replace("+ 0\n", "+ 7\n")).into_diagnostic()?;
    let body = project.submit()?;
    table.push(report("body edit", &body));

    let text = std::fs::read_to_string(source(middle)).into_diagnostic()?;
    std::fs::write(
        source(middle),
        text.replace(
            &format!("pub fn f{middle}_1(x: u32) -> u32"),
            &format!("pub fn f{middle}_1(x: u32) -> u64"),
        ),
    )
    .into_diagnostic()?;
    let signature = project.submit()?;
    table.push(report("signature edit", &signature));

    tracing::warn!(
        "{files} modules x {per_file} functions\n{}",
        table.join("\n")
    );
    for (name, row) in [
        ("comment", &commented),
        ("body", &body),
        ("signature", &signature),
    ] {
        assert_eq!(
            (
                row["loaded"].as_str(),
                row["kept"].as_str(),
                row["reloaded"].as_str()
            ),
            ("0", "1", "0"),
            "{name} edit should reuse the database: {row:?}"
        );
        assert!(
            seconds(row, "wall_s") < seconds(&cold, "wall_s") / 2.0,
            "{name} edit should cost well under the cold load\n{}",
            table.join("\n")
        );
    }
    assert_eq!(
        signature["findings"],
        project.plain_findings("final")?,
        "after all the edits the session must still agree with a plain run"
    );
    project.run(&["session", "stop"], true)?;
    Ok(())
}
