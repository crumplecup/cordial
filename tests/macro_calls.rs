//! Calls inside macro invocations are seen through when rust-analyzer can expand
//! the macro, and the function stays marked incomplete either way.

use cordial::{
    CrateIr, CrateTarget, EdgeKind, IrView, NodeId, SemanticCallTarget, SemanticFunction,
    SourceDatabase,
};
use miette::{IntoDiagnostic, Result};

const LIBRARY: &str = include_str!("fixtures/macro_calls/lib.rs.txt");

fn project() -> Result<(tempfile::TempDir, CrateIr)> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = database
        .project(
            &CrateTarget::new("macros", root.path()),
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

fn facts(graph: &CrateIr, name: &str) -> Result<SemanticFunction> {
    graph
        .semantic_function(node(graph, name)?)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("`{name}` has no semantic facts"))
}

fn callees(graph: &CrateIr, name: &str) -> Result<Vec<NodeId>> {
    Ok(graph
        .neighbors(
            node(graph, name)?,
            EdgeKind::Calls,
            petgraph::Direction::Outgoing,
        )
        .into_iter()
        .map(|(_, id)| id)
        .collect())
}

fn calls_target(graph: &CrateIr, name: &str) -> Result<usize> {
    let target = *facts(graph, "target")?.definition();
    Ok(facts(graph, name)?
        .calls()
        .iter()
        .filter(|call| call.target() == &SemanticCallTarget::Function(target))
        .count())
}

#[test]
fn a_call_in_an_assert_is_a_call_of_the_function() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    assert_eq!(calls_target(&graph, "asserts")?, 1);
    assert_eq!(callees(&graph, "asserts")?, vec![node(&graph, "target")?]);
    assert!(*facts(&graph, "asserts")?.incomplete());
    Ok(())
}

#[test]
fn calls_in_a_local_macro_expansion_join_the_graph() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    assert_eq!(calls_target(&graph, "local_macro")?, 2);
    assert_eq!(
        callees(&graph, "local_macro")?,
        vec![node(&graph, "target")?]
    );
    assert!(*facts(&graph, "local_macro")?.incomplete());
    Ok(())
}

#[test]
fn a_macro_that_cannot_be_expanded_stays_a_gap() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    assert_eq!(calls_target(&graph, "unexpandable")?, 0);
    assert!(callees(&graph, "unexpandable")?.is_empty());
    assert!(*facts(&graph, "unexpandable")?.incomplete());
    Ok(())
}

#[test]
fn a_plain_call_is_unchanged_and_complete() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    assert_eq!(calls_target(&graph, "plain")?, 1);
    assert!(!*facts(&graph, "plain")?.incomplete());
    Ok(())
}
