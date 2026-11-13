//! The whole pipeline through a session, on a small project: after an edit the
//! session reuses its database instead of loading again, a change it cannot follow
//! reloads, and the findings always equal a plain process run on the same tree.
//! The run log says which of those happened, so this reads that.

mod support {
    pub mod mini_workspace;
}
use cordial::host::socket_path;
use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Project {
    project: PathBuf,
    home: PathBuf,
}

impl Project {
    fn cordial(&self, args: &[&str], session: bool) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cordial"));
        command
            .arg("--project")
            .arg(&self.project)
            .arg("--store-home")
            .arg(&self.home)
            .args(args)
            .env("CORDIAL_NO_PROGRESS", "1")
            .stdin(Stdio::null());
        if session {
            command.env_remove("CORDIAL_NO_SESSION");
        } else {
            command.env("CORDIAL_NO_SESSION", "1");
        }
        command
    }

    fn run(&self, args: &[&str], session: bool) -> Result<Output> {
        let output = self.cordial(args, session).output().into_diagnostic()?;
        assert!(
            output.status.success(),
            "cordial {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    }

    /// The run log's rows, each as column name to cell.
    fn rows(&self) -> Result<Vec<BTreeMap<String, String>>> {
        let log = self.home.join("mini/perf/runs.csv");
        let text = std::fs::read_to_string(&log).into_diagnostic()?;
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next().unwrap_or_default().split(',').collect();
        Ok(lines
            .map(|line| {
                header
                    .iter()
                    .zip(line.split(','))
                    .map(|(name, cell)| ((*name).to_owned(), cell.to_owned()))
                    .collect()
            })
            .collect())
    }

    fn last(&self) -> Result<BTreeMap<String, String>> {
        self.rows()?
            .pop()
            .ok_or_else(|| miette::miette!("no run was logged"))
    }

    /// `submit` in the session, then the same tree again in a plain process with its
    /// own empty store: the findings must be the same.
    fn submit_and_check(&self, label: &str) -> Result<BTreeMap<String, String>> {
        self.run(&["submit", "--tier", "build"], true)?;
        let session = self.last()?;
        let plain = Project {
            project: self.project.clone(),
            // A store of its own each time: `open` reuses a list it already holds, and
            // the comparison must come from a full run of this exact tree.
            home: self
                .home
                .with_file_name(format!("plain-{}", label.replace(' ', "-"))),
        };
        plain.run(&["open"], false)?;
        assert_eq!(
            session["findings"],
            plain.last()?["findings"],
            "{label}: the session's findings differ from a plain run"
        );
        Ok(session)
    }
}

/// The named cells of a row, in the order asked.
fn pick<'a>(row: &'a BTreeMap<String, String>, names: &[&str]) -> Vec<&'a str> {
    names.iter().map(|name| row[*name].as_str()).collect()
}

fn append(path: &Path, text: &str) -> Result<()> {
    let mut contents = std::fs::read_to_string(path).into_diagnostic()?;
    contents.push_str(text);
    std::fs::write(path, contents).into_diagnostic()
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
    let unchanged = project.submit_and_check("unchanged")?;
    assert_eq!(unchanged["cache_hits"], "1", "{unchanged:?}");
    assert_eq!(unchanged["loaded"], "0");

    let cold: f64 = first["wall_s"].parse().into_diagnostic()?;

    // A comment, then a body edit, then a new function: each is followed in place.
    append(&other, "\n// a comment\n")?;
    let commented = project.submit_and_check("comment")?;
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
    let added = project.submit_and_check("new function")?;
    assert_eq!(
        pick(&added, &["loaded", "kept", "reloaded"]),
        ["0", "1", "0"],
        "{added:?}"
    );

    let text = std::fs::read_to_string(&lib).into_diagnostic()?;
    std::fs::write(&lib, text.replace("+ 1", "+ 2")).into_diagnostic()?;
    let body = project.submit_and_check("body edit")?;
    assert_eq!(pick(&body, &["loaded", "kept"]), ["0", "1"], "{body:?}");

    // A source file the database never loaded cannot be followed: it reloads.
    std::fs::write(project.project.join("src/extra.rs"), "pub fn extra() {}\n")
        .into_diagnostic()?;
    append(&lib, "mod extra;\n")?;
    let reloaded = project.submit_and_check("new file")?;
    assert_eq!(
        pick(&reloaded, &["reloaded", "loaded"]),
        ["1", "1"],
        "{reloaded:?}"
    );

    project.run(&["session", "stop"], true)?;
    Ok(())
}
