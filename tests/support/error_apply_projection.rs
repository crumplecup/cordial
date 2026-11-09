//! Exercise the production apply collector by loading a graph's source fresh.
use super::apply_source::materialized_with_source;
use cordial::CrateIr;
use cordial::{CrateTarget, LoadContext, Loader, SessionBuilder, SourceLoadView, SourceLoader};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
pub fn materialized(graph: CrateIr, root: &Path) -> Result<serde_json::Value> {
    let target = CrateTarget::new(graph.crate_name(), root);
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    materialized_with_source(graph, source, &session)
}
