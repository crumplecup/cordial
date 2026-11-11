//! Project a target from a database loaded over its Cargo workspace.
use cordial::{CrateIr, CrateTarget, SourceDatabase};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(target), err(level = "warn"))]
pub fn baseline(root: &Path, target: &CrateTarget) -> Result<CrateIr> {
    SourceDatabase::load(root)
        .into_diagnostic()?
        .project_target(target)
        .into_diagnostic()
}
