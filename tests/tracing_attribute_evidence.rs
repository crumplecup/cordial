//! Missing attribute evidence must stop assessment rather than invent edits.

use cordial::{
    AssessView, CrateIr, Etiquette, FileSpan, IrView, ItemKind, NodeKind, NodeWeight, ProbeView,
    SessionBuilder, TRACING_ETIQUETTE,
};
use miette::{IntoDiagnostic, Result};

#[path = "support/tracing_inventory_consumer.rs"]
mod tracing_inventory_consumer;

#[test]
fn enrichment_uses_loader_inventory_and_rejects_incomplete_facts() -> Result<()> {
    use cordial::Session;
    cordial::init_cli_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"inventory_consumer\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n").into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), "pub fn work() {}\n").into_diagnostic()?;
    for corrupt in [false, true] {
        let session = SessionBuilder::new(root.path())
            .with_store_root(store.path())
            .register(tracing_inventory_consumer::etiquette(corrupt))
            .build();
        let result = session.run(&cordial::RunAll);
        if corrupt {
            assert!(result.is_err(), "incomplete facts must stop the pipeline");
        } else {
            result.into_diagnostic()?;
        }
    }
    Ok(())
}

#[test]
fn recipe_assessment_rejects_missing_attribute_evidence() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(root.path()).build();
    let mut graph = CrateIr::new("fixture");
    let mut node = NodeWeight::new(NodeKind::Item(ItemKind::Fn)).with_name("stable");
    for (key, value) in [
        ("function_kind", "free"),
        ("qualified_path", "stable"),
        ("file", "src/lib.rs"),
        ("recipe_level", "trace"),
    ] {
        node.set_attr(key, value.into());
    }
    node.set_attr("instrumented", true.into());
    node.set_attr("line", 1.into());
    graph.insert_node(node);
    let probe = TRACING_ETIQUETTE
        .probes()
        .iter()
        .find(|probe| probe.id() == "recipe-delta")
        .ok_or_else(|| miette::miette!("missing recipe probe"))?;
    let markers = probe
        .probe(ProbeView::new(&graph, &session))
        .into_diagnostic()?;
    assert_eq!(markers.len(), 1);
    let refs: Vec<_> = markers.iter().map(|marker| marker.as_ref()).collect();
    let assessor = TRACING_ETIQUETTE
        .assessors()
        .iter()
        .find(|assessor| assessor.id() == "tracing-assessor")
        .ok_or_else(|| miette::miette!("missing tracing assessor"))?;
    let result = assessor.assess(AssessView::new(&refs, &graph, &session));
    assert!(
        result.is_err(),
        "missing HasAttr evidence must not become default instrument arguments"
    );
    Ok(())
}

#[test]
fn declaration_lookup_preserves_alternatives_through_snapshot_restore() -> Result<()> {
    cordial::init_tracing();
    let file = std::path::Path::new("src/lib.rs");
    let sibling = std::path::Path::new("src/alternate.rs");
    let mut graph = CrateIr::new("fixture");
    let mut declarations = Vec::new();
    for (path, line) in [(file, 1), (file, 5), (sibling, 1)] {
        let mut node = NodeWeight::new(NodeKind::Item(ItemKind::Fn))
            .with_name("choose")
            .with_span(FileSpan::new(path, line, 1));
        node.set_attr("qualified_path", "choose".into());
        let id = graph.insert_node(node);
        // Attribute updates must not duplicate a declaration in the index.
        graph
            .set_attr(id, "instrumented", true.into())
            .into_diagnostic()?;
        declarations.push(id);
    }
    let snapshot = graph.snapshot().into_diagnostic()?;
    let restored = CrateIr::from_snapshot(snapshot).into_diagnostic()?;
    for graph in [&graph, &restored] {
        assert_eq!(graph.nodes_by_path("choose").len(), 3);
        for ((path, line), expected) in [(file, 1), (file, 5), (sibling, 1)]
            .into_iter()
            .zip(&declarations)
        {
            assert_eq!(
                graph
                    .source_node_at("choose", path, line)
                    .into_diagnostic()?,
                Some(*expected)
            );
        }
        assert_eq!(
            graph.source_node_at("choose", file, 9).into_diagnostic()?,
            None
        );
    }
    // A line number cannot distinguish two declarations on one physical line.
    // Require richer evidence instead of choosing an arbitrary sibling.
    let mut ambiguous =
        NodeWeight::new(NodeKind::Item(ItemKind::Fn)).with_span(FileSpan::new(file, 1, 1));
    ambiguous.set_attr("qualified_path", "choose".into());
    graph.insert_node(ambiguous);
    assert!(graph.source_node_at("choose", file, 1).is_err());
    Ok(())
}
