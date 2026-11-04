//! Graph-only tracing policy execution, deliberately supplying an empty source view.
use cordial::{
    CrateIr, CrateTarget, CrateViewMut, EnrichView, Etiquette, IrView, LoadContext, Loader,
    QueryBuilder, SessionBuilder, SourceLoadView, SourceLoader, TRACING_ETIQUETTE, WorkspaceIr,
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
        .ok_or_else(|| miette::miette!("source loader"))?;
    let mut graph = CrateIr::new(target.crate_name());
    source.populate_ir(&mut graph).into_diagnostic()?;
    Ok(graph)
}

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
pub fn findings(graph: &CrateIr, root: &Path) -> Result<Vec<serde_json::Value>> {
    let empty = tempfile::tempdir().into_diagnostic()?;
    let target = CrateTarget::new("empty", empty.path());
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source loader"))?;
    assert!(source.files().is_empty());
    let mut workspace = WorkspaceIr::default();
    workspace.insert_crate(graph.clone());
    let mut view = CrateViewMut::new(&mut workspace, graph.crate_name().into());
    for hook in TRACING_ETIQUETTE.enrichers().iter().filter(|hook| {
        matches!(
            hook.id(),
            "tracing-subscriber-inventory"
                | "tracing-boundary-inventory"
                | "tracing-print-inventory"
        )
    }) {
        hook.enrich(EnrichView::new(&mut view, source, &session))
            .into_diagnostic()?;
    }
    let mut rows = Vec::new();
    for key in ["subscriber_rule_id", "boundary_rule_id", "print_rule_id"] {
        for node in view.nodes_matching(&QueryBuilder::new().has_attr(key).build()) {
            let attrs: std::collections::BTreeMap<_, _> =
                node.weight().attrs().iter().cloned().collect();
            rows.push(serde_json::json!({"attrs": attrs, "span": node.weight().span()}));
        }
    }
    rows.sort_by_key(serde_json::Value::to_string);
    Ok(rows)
}
