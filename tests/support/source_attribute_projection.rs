//! Exercise the attribute consumer with no source files supplied to its hook.

use cordial::{
    AttributeEnricher, CrateIr, CrateTarget, EdgeKind, IrView, ItemKind, LoadContext, Loader,
    NodeKind, NodeWeight, QueryBuilder, SessionBuilder, SourceEnrichView, SourceEnricher,
    SourceLoadView, SourceLoader, SourceSpan,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
pub fn assert_graph_only_attributes(graph: &CrateIr, root: &Path) -> Result<()> {
    let mut graph = graph.clone();
    let module = graph.nodes_matching(
        &QueryBuilder::new()
            .has_attr(cordial::ATTR_SOURCE_FILE_ATTRIBUTES)
            .build(),
    )[0]
    .id();
    let facts = graph
        .source_file_attributes(module)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing attribute facts"))?;
    // Other collectors normally create methods and nested owners. Supply their
    // graph identities here so this assertion isolates attribute materialization.
    for item in facts.items() {
        let owner = match graph.node_by_path(item.path()) {
            Some(owner) => owner,
            None => {
                let mut node = NodeWeight::new(NodeKind::Item(ItemKind::Fn))
                    .with_name(item.path().clone())
                    .with_span(cordial::FileSpan::new(
                        root.join("src/lib.rs"),
                        item.line(),
                        1,
                    ));
                node.set_attr("qualified_path", item.path().clone().into());
                graph.insert_node(node)
            }
        };
        graph
            .set_attr(owner, "function_kind", "free".into())
            .into_diagnostic()?;
    }
    let empty = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(root).build();
    let target = CrateTarget::new("empty", empty.path());
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("unexpected loader"))?;
    assert!(source.files().is_empty());
    AttributeEnricher
        .enrich_source(SourceEnrichView::new(&mut graph, source, &session))
        .into_diagnostic()?;
    for item in facts.items() {
        let owner = graph
            .node_by_path(item.path())
            .ok_or_else(|| miette::miette!("missing owner"))?;
        let mut actual = graph
            .children(owner, EdgeKind::HasAttr)
            .into_iter()
            .map(|id| {
                let node = graph
                    .node(id)
                    .ok_or_else(|| miette::miette!("missing attribute node"))?;
                let span = node
                    .weight()
                    .span()
                    .as_ref()
                    .ok_or_else(|| miette::miette!("missing attribute span"))?;
                Ok((
                    node.attr("attr_path").cloned(),
                    node.attr("meta").cloned(),
                    span.file().to_path_buf(),
                    span.line(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut expected: Vec<_> = item
            .attributes()
            .iter()
            .map(|attr| {
                (
                    Some(attr.path().clone().into()),
                    Some(attr.meta().clone().into()),
                    root.join("src/lib.rs"),
                    item.line(),
                )
            })
            .collect();
        // Graph adjacency order is not source attribute order.
        actual.sort_by_key(|entry| format!("{entry:?}"));
        expected.sort_by_key(|entry| format!("{entry:?}"));
        assert_eq!(
            actual,
            expected,
            "graph-only attributes for {}",
            item.path()
        );
    }
    Ok(())
}
