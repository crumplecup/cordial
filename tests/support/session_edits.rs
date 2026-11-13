//! Editing a project between runs and running `submit` in its session.

use super::session_project::{Project, Row};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

/// `submit` in the session and the row it logged.
pub(crate) fn submit(project: &Project) -> Result<Row> {
    project.run(&["submit", "--tier", "build"], true)?;
    project.last()
}

/// Append `text` to the file at `path`.
pub(crate) fn append(path: &Path, text: &str) -> Result<()> {
    let mut contents = std::fs::read_to_string(path).into_diagnostic()?;
    contents.push_str(text);
    std::fs::write(path, contents).into_diagnostic()
}
