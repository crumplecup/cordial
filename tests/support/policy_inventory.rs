//! The policy inventory the loader stores for a crate on disk.
use cordial::{IrView, SourcePolicyInventory};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

#[path = "source_projection.rs"]
mod source_projection;

/// Project the first crate target under `root` through the database and read
/// back its policy inventory.
#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn policy_inventory(root: &Path) -> Result<SourcePolicyInventory> {
    let target = cordial::discover_crate_targets(root, &cordial::RunAll)
        .into_diagnostic()?
        .into_iter()
        .next()
        .ok_or_else(|| miette::miette!("no crate target under {}", root.display()))?;
    source_projection::baseline(root, &target)?
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no policy inventory for {}", root.display()))
}
