//! The crate graph the loader stores for the first target under a root.
use cordial::CrateIr;
use miette::{IntoDiagnostic, Result};
use std::path::Path;

#[path = "source_projection.rs"]
mod source_projection;

/// Project the first crate target under `root` through the database.
#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn projected_crate(root: &Path) -> Result<CrateIr> {
    let target = cordial::discover_crate_targets(root, &cordial::RunAll)
        .into_diagnostic()?
        .into_iter()
        .next()
        .ok_or_else(|| miette::miette!("no crate target under {}", root.display()))?;
    source_projection::baseline(root, &target)
}
