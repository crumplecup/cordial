//! Construct the compatibility loader projection for complete payload parity.
use cordial::{
    CrateIr, CrateTarget, LoadContext, Loader, SessionBuilder, SourceLoadView, SourceLoader,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(target), err(level = "warn"))]
pub fn baseline(root: &Path, target: &CrateTarget) -> Result<CrateIr> {
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    let mut graph = CrateIr::new(target.crate_name());
    source.populate_ir(&mut graph).into_diagnostic()?;
    Ok(graph)
}
