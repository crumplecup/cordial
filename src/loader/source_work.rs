//! Tracing and hotpath boundary for reading Rust source text.

use std::path::Path;

use crate::error::CordialResult;

#[hotpath::measure]
#[tracing::instrument(level = "debug", skip(path), fields(path = %path.display()))]
pub(crate) fn read(path: &Path, hook: &str) -> CordialResult<String> {
    let source = std::fs::read_to_string(path)?;
    tracing::trace!("source read");
    Ok(source)
}
