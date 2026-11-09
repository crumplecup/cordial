//! Exercise the production apply collector with a supplied loader source view.
use cordial::{
    CrateIr, ErrorIrScanEnricher, IrView, QueryBuilder, SessionView, SourceEnrichView,
    SourceEnricher, SourceLoadView,
};
use miette::{IntoDiagnostic, Result};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

#[tracing::instrument(level = "debug", skip(graph, source, session), err(level = "warn"))]
pub fn materialized_with_source(
    mut graph: CrateIr,
    source: &SourceLoadView,
    session: &dyn SessionView,
) -> Result<serde_json::Value> {
    let syntax = SyntaxRequests::default();
    let subscriber = tracing_subscriber::registry()
        .with(syntax.clone())
        .with(cordial::tracing_fmt_layer());
    let _subscriber = tracing::subscriber::set_default(subscriber);
    ErrorIrScanEnricher
        .enrich_source(SourceEnrichView::new(&mut graph, source, session))
        .into_diagnostic()?;
    assert_eq!(
        syntax.0.load(Ordering::Relaxed),
        0,
        "apply enrichment must not request compatibility syntax"
    );
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

/// Observe the existing loader boundary without introducing a production counter.
#[derive(Clone, Default)]
struct SyntaxRequests(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> Layer<S> for SyntaxRequests {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _id: &tracing::Id,
        _context: Context<'_, S>,
    ) {
        let metadata = attributes.metadata();
        if metadata.target() == "cordial::loader::source::cache" && metadata.name() == "parsed_file"
        {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}
