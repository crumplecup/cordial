//! The whole pipeline through a session, on a small project: after an edit the
//! session reuses its database instead of loading again, a change it cannot follow
//! reloads, and the findings always equal a plain process run on the same tree.
//! The run log says which of those happened, so this reads that.

mod support {
    pub mod mini_workspace;
    pub mod session_edits;
    pub mod session_project;
}
use cordial::host::socket_path;
use miette::{IntoDiagnostic, Result};
use support::session_edits::{append, submit};
use support::session_project::{Project, Row};

/// `submit` in the session, then a plain run on the same tree: the findings must
/// be the same.
fn submit_and_check(project: &Project, label: &str) -> Result<Row> {
    let session = submit(project)?;
    assert_eq!(
        session["findings"],
        project.plain_findings(label)?,
        "{label}: the session's findings differ from a plain run"
    );
    Ok(session)
}

/// The named cells of a row, in the order asked.
fn pick<'a>(row: &'a Row, names: &[&str]) -> Vec<&'a str> {
    names.iter().map(|name| row[*name].as_str()).collect()
}

#[test]
fn a_session_reuses_its_database_after_an_edit_and_agrees_with_a_plain_run() -> Result<()> {
    cordial::init_tracing();
    let scratch = tempfile::tempdir().into_diagnostic()?;
    let project = Project {
        project: scratch.path().join("mini"),
        home: scratch.path().join("store"),
    };
    support::mini_workspace::write(&project.project)?;
    let other = project.project.join("src/other.rs");
    let lib = project.project.join("src/lib.rs");

    project.run(&["session", "start"], true)?;
    assert!(socket_path(&project.home.join("mini")).exists());

    // The first run in a session loads the workspace; nothing is kept yet.
    project.run(&["open"], true)?;
    let first = project.last()?;
    assert_eq!(first["mode"], "session");
    assert_eq!(pick(&first, &["loaded", "kept"]), ["1", "0"]);

    // Nothing changed: the run cache answers, with no database at all.
    let unchanged = submit_and_check(&project, "unchanged")?;
    let said = &unchanged["cache"];
    assert!(
        said.contains("crate IR mini: reused from the run cache")
            && said.contains("call graph: reused from the run cache"),
        "{unchanged:?}"
    );
    assert_eq!(unchanged["loaded"], "0");

    let cold: f64 = first["wall_s"].parse().into_diagnostic()?;

    // A comment, then a body edit, then a new function: each is followed in place.
    append(&other, "\n// a comment\n")?;
    let commented = submit_and_check(&project, "comment")?;
    assert_eq!(
        pick(&commented, &["loaded", "kept", "files_changed"]),
        ["0", "1", "1"],
        "a comment edit should reuse the database: {commented:?}"
    );

    let commented_wall: f64 = commented["wall_s"].parse().into_diagnostic()?;
    tracing::warn!(
        cold,
        edit = commented_wall,
        "cold load against an edit in the session"
    );
    assert!(
        commented_wall < cold / 2.0,
        "an edit should cost well under the cold load: {commented_wall}s against {cold}s"
    );

    append(&other, "pub fn added() -> u8 { 3 }\n")?;
    let added = submit_and_check(&project, "new function")?;
    assert_eq!(
        pick(&added, &["loaded", "kept", "reloaded"]),
        ["0", "1", "0"],
        "{added:?}"
    );

    let text = std::fs::read_to_string(&lib).into_diagnostic()?;
    std::fs::write(&lib, text.replace("+ 1", "+ 2")).into_diagnostic()?;
    let body = submit_and_check(&project, "body edit")?;
    assert_eq!(pick(&body, &["loaded", "kept"]), ["0", "1"], "{body:?}");

    // A source file the database never loaded cannot be followed: it reloads.
    std::fs::write(project.project.join("src/extra.rs"), "pub fn extra() {}\n")
        .into_diagnostic()?;
    append(&lib, "mod extra;\n")?;
    let reloaded = submit_and_check(&project, "new file")?;
    assert_eq!(
        pick(&reloaded, &["reloaded", "loaded"]),
        ["1", "1"],
        "{reloaded:?}"
    );

    project.run(&["session", "stop"], true)?;
    Ok(())
}
