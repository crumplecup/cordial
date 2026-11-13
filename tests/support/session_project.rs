//! Driving the real binary against a project in a store of its own, and reading
//! the run log it appends to.

use miette::{IntoDiagnostic, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// One log row: column name to cell.
pub(crate) type Row = BTreeMap<String, String>;

/// A project and the store its runs write to.
pub(crate) struct Project {
    pub(crate) project: PathBuf,
    pub(crate) home: PathBuf,
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

    /// Run `args`, through the session when there is one and `session` is set.
    pub(crate) fn run(&self, args: &[&str], session: bool) -> Result<Output> {
        let output = self.cordial(args, session).output().into_diagnostic()?;
        assert!(
            output.status.success(),
            "cordial {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output)
    }

    /// The last run the store logged.
    pub(crate) fn last(&self) -> Result<Row> {
        let log = self.home.join("mini/perf/runs.csv");
        let text = std::fs::read_to_string(&log).into_diagnostic()?;
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next().unwrap_or_default().split(',').collect();
        lines
            .next_back()
            .map(|line| {
                header
                    .iter()
                    .zip(line.split(','))
                    .map(|(name, cell)| ((*name).to_owned(), cell.to_owned()))
                    .collect()
            })
            .ok_or_else(|| miette::miette!("no run was logged"))
    }

    /// The findings a plain process finds on this tree, in a store of its own so
    /// that it runs the whole pipeline instead of reusing a list.
    pub(crate) fn plain_findings(&self, label: &str) -> Result<String> {
        let plain = Project {
            project: self.project.clone(),
            home: self
                .home
                .with_file_name(format!("plain-{}", label.replace(' ', "-"))),
        };
        plain.run(&["open"], false)?;
        Ok(plain.last()?["findings"].clone())
    }
}
