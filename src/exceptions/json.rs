//! JSON file helpers shared by exception persistence paths.

use std::fs;
use std::path::Path;

use serde::Serialize;
use tracing::instrument;

use crate::error::{CordialError, CordialResult};

use super::entry::ExceptionEntry;

#[instrument(level = "debug", skip(path), err(level = "warn"))]
pub(super) fn parse_exception_file(path: &Path) -> CordialResult<Vec<ExceptionEntry>> {
    let bytes = fs::read(path)?;
    serde_json::from_slice::<Vec<ExceptionEntry>>(&bytes)
        .map_err(|err| crate::error::CordialError::json_parse(path.display().to_string(), err))
}

#[instrument(level = "debug", skip(path), err(level = "warn"))]
pub(super) fn parse_json_array(path: &Path) -> CordialResult<Vec<serde_json::Value>> {
    let bytes = fs::read(path)?;
    serde_json::from_slice::<Vec<serde_json::Value>>(&bytes)
        .map_err(|err| CordialError::json_parse(path.display().to_string(), err))
}

#[instrument(level = "debug", skip(rows, path))]
pub(super) fn json_rows_contain_path(rows: &[serde_json::Value], path: &str) -> bool {
    rows.iter().any(|row| {
        row.get("path")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value == path)
    })
}

#[instrument(level = "debug", skip(path, value), err(level = "warn"))]
pub(super) fn write_pretty_json(path: &Path, value: &impl Serialize) -> CordialResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut body = serde_json::to_string_pretty(value)?;
    if !body.ends_with('\n') {
        body.push('\n');
    }
    fs::write(path, body)?;
    Ok(())
}
