//! Panic sites of one source text, read from the database-backed inventory.
use cordial::{IrView, PanicSiteRecord};
use miette::{IntoDiagnostic, Result, WrapErr};

#[path = "source_fixture.rs"]
mod source_fixture;

/// The panic records the loader projects for `source` as a crate lib.
#[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
pub fn panic_sites(source: &str) -> Result<Vec<PanicSiteRecord>> {
    Ok(source_fixture::project_source(source)?
        .source_panics()
        .into_diagnostic()
        .wrap_err("panic inventory")?
        .records()
        .clone())
}
