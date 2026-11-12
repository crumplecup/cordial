//! A function an attribute macro generated, as every `#[instrument]`ed one is, still
//! has the calls it makes. They carry no range that could be mistaken for a call
//! written in the source, and the function stays incomplete.

use cordial::{CrateIr, CrateTarget, EdgeKind, IrView, NodeId, SourceDatabase};
use miette::{IntoDiagnostic, Result};

const LIBRARY: &str = include_str!("fixtures/instrumented_calls/lib.rs.txt");

fn project() -> Result<(tempfile::TempDir, CrateIr)> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='instrumented'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ntracing='0.1'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = database
        .project(
            &CrateTarget::new("instrumented", root.path()),
            &[root.path().join("src/lib.rs")],
        )
        .into_diagnostic()?;
    Ok((root, graph))
}

fn node(graph: &CrateIr, name: &str) -> Result<NodeId> {
    graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .find(|node| node.weight().name().as_deref() == Some(name))
        .map(|node| node.id())
        .ok_or_else(|| miette::miette!("no function `{name}`"))
}

#[test]
fn an_instrumented_function_has_its_calls_and_its_edge() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    let facts = graph
        .semantic_function(node(&graph, "instrumented")?)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no facts"))?;
    assert!(
        facts.calls().iter().any(|call| call
            .callee()
            .as_ref()
            .is_some_and(|callee| callee.path() == "instrumented::target")),
        "{:?}",
        facts.calls()
    );
    assert!(*facts.incomplete());
    assert_eq!(
        graph
            .neighbors(
                node(&graph, "instrumented")?,
                EdgeKind::Calls,
                petgraph::Direction::Outgoing
            )
            .into_iter()
            .map(|(_, id)| id)
            .collect::<Vec<_>>(),
        [node(&graph, "target")?]
    );
    Ok(())
}

#[test]
fn the_calls_of_a_generated_function_match_no_written_call() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    let facts = graph
        .semantic_function(node(&graph, "instrumented")?)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no facts"))?;
    let written = LIBRARY.len();
    assert!(
        facts
            .calls()
            .iter()
            .all(|call| call.range().start() > written),
        "{:?}",
        facts.calls()
    );
    Ok(())
}

#[test]
fn a_plain_function_keeps_its_written_range() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    let facts = graph
        .semantic_function(node(&graph, "plain")?)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no facts"))?;
    let call = &facts.calls()[0];
    assert!(call.range().end() <= LIBRARY.len());
    assert!(!*facts.incomplete());
    Ok(())
}
