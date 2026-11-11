//! The antipattern findings the loader's inventory gives a member crate.

use cordial::{
    AntipatternSiteRecord, AntipatternWorkspaceFacts, CrateTarget, ErrorFieldStrategy, IrView,
    StaticRefStrategy,
};
use miette::IntoDiagnostic;
use std::path::Path;

#[path = "source_projection.rs"]
mod source_projection;

/// The antipattern findings the loader's inventory gives the member crate,
/// with registry and workspace-version evidence from `workspace_root`.
pub fn crate_antipatterns(
    member: &Path,
    name: &str,
    workspace_root: &Path,
) -> miette::Result<Vec<AntipatternSiteRecord>> {
    let facts =
        AntipatternWorkspaceFacts::load(workspace_root, workspace_root).into_diagnostic()?;
    source_projection::baseline(workspace_root, &CrateTarget::new(name, member))?
        .source_antipatterns()
        .into_diagnostic()?
        .assess(
            name,
            &facts,
            StaticRefStrategy::default(),
            ErrorFieldStrategy::default(),
        )
        .into_diagnostic()
}
