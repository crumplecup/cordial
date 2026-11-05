//! Run the production site materializer with an empty source view.

use cordial::{
    CrateIr, CrateTarget, CrateViewMut, ERROR_IR_ENRICHERS, EnrichView, IrView, LoadContext,
    Loader, QueryBuilder, SessionBuilder, SourceLoadView, SourceLoader, WorkspaceIr,
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

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
pub fn materialized(graph: &CrateIr, root: &Path) -> Result<Vec<serde_json::Value>> {
    materialized_with(graph, root, &["error-site-inventory"], "error_site_kind")
}

#[tracing::instrument(level = "debug", skip(graph, hooks), err(level = "warn"))]
pub fn materialized_with(
    graph: &CrateIr,
    root: &Path,
    hooks: &[&str],
    attribute: &str,
) -> Result<Vec<serde_json::Value>> {
    let empty = tempfile::tempdir().into_diagnostic()?;
    let target = CrateTarget::new("empty", empty.path());
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    assert!(source.files().is_empty());
    let mut workspace = WorkspaceIr::default();
    workspace.insert_crate(graph.clone());
    let mut view = CrateViewMut::new(&mut workspace, graph.crate_name().into());
    for id in hooks {
        let hook = ERROR_IR_ENRICHERS
            .iter()
            .find(|hook| hook.id() == *id)
            .ok_or_else(|| miette::miette!("missing hook {id}"))?;
        hook.enrich(EnrichView::new(&mut view, source, &session))
            .into_diagnostic()?;
    }
    let mut rows = Vec::new();
    for node in view.nodes_matching(&QueryBuilder::new().has_attr(attribute).build()) {
        let attrs: std::collections::BTreeMap<_, _> =
            node.weight().attrs().iter().cloned().collect();
        rows.push(serde_json::json!({"attrs": attrs, "span": node.weight().span()}));
    }
    rows.sort_by_key(serde_json::Value::to_string);
    Ok(rows)
}
