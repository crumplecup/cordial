//! Exercise the production apply collector with supplied loader evidence.
use cordial::{
    CrateIr, CrateTarget, ErrorIrScanEnricher, IrView, LoadContext, Loader, QueryBuilder,
    SessionBuilder, SourceEnrichView, SourceEnricher, SourceLoadView, SourceLoader,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
pub fn materialized(mut graph: CrateIr, root: &Path) -> Result<serde_json::Value> {
    let target = CrateTarget::new(graph.crate_name(), root);
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    ErrorIrScanEnricher
        .enrich_source(SourceEnrichView::new(&mut graph, source, &session))
        .into_diagnostic()?;
    let mut units = graph
        .nodes_matching(
            &QueryBuilder::new()
                .has_attr("error_conversion_unit")
                .build(),
        )
        .into_iter()
        .map(|node| {
            node.attr("error_conversion_unit")
                .cloned()
                .ok_or_else(|| miette::miette!("conversion unit"))
        })
        .collect::<Result<Vec<_>>>()?;
    units.sort_by_key(serde_json::Value::to_string);
    let root = graph
        .node(graph.root())
        .ok_or_else(|| miette::miette!("root node"))?;
    let mut index = root
        .attr("error_apply_type_index")
        .cloned()
        .ok_or_else(|| miette::miette!("type index"))?;
    // These arrays encode maps; source-order arrays inside the facts stay ordered.
    for key in [
        "declarations",
        "local",
        "modules",
        "imports",
        "file_syntax",
        "incoming_edges",
    ] {
        if let Some(entries) = index[key].as_array_mut() {
            entries.sort_by_key(serde_json::Value::to_string);
        }
    }
    let plans = root
        .attr("error_apply_graph_plans")
        .cloned()
        .ok_or_else(|| miette::miette!("graph plans"))?;
    Ok(serde_json::json!({"index": index, "plans": plans, "units": units}))
}
