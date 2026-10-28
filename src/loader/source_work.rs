//! Tracing and hotpath boundaries for shared Rust source work.

use std::path::Path;

use crate::error::{CordialError, CordialResult};

#[hotpath::measure]
#[tracing::instrument(level = "debug", skip(path), fields(path = %path.display()))]
pub(crate) fn read(path: &Path, hook: &str) -> CordialResult<String> {
    let source = std::fs::read_to_string(path)?;
    tracing::trace!("source read");
    Ok(source)
}

#[hotpath::measure]
#[tracing::instrument(level = "debug", skip(source, path), fields(path = %path.display()))]
pub(crate) fn parse(source: &str, path: &Path, hook: &str) -> CordialResult<syn::File> {
    let syntax = syn::parse_file(source)
        .map_err(|error| CordialError::syn_parse(path.display().to_string(), error))?;
    tracing::trace!("source parsed");
    Ok(syntax)
}

#[hotpath::measure]
#[tracing::instrument(level = "debug", skip(path, action), fields(path = %path.display()))]
pub(crate) fn walk<T>(path: &Path, hook: &str, action: impl FnOnce() -> T) -> T {
    let result = action();
    tracing::trace!("source walked");
    result
}

#[tracing::instrument(level = "trace", skip(path), fields(path = %path.display()))]
pub(crate) fn reuse(path: &Path, hook: &str) {
    tracing::trace!("parsed source reused");
}
