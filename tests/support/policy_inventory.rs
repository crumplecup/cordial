//! The policy inventory the loader stores for a crate on disk.
use cordial::{IrView, SourcePolicyInventory};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

#[path = "projected_crate.rs"]
mod projected_crate;

/// Read back the policy inventory of the first crate target under `root`.
#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn policy_inventory(root: &Path) -> Result<SourcePolicyInventory> {
    projected_crate::projected_crate(root)?
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no policy inventory for {}", root.display()))
}
