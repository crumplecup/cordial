//! A function inside a macro invocation that rust-analyzer cannot expand is a
//! node of its own: gated by its attributes, with its calls resolved in the
//! invocation's scope.

use cordial::{CrateIr, CrateTarget, EdgeKind, IrView, NodeId, SourceDatabase};
use miette::{IntoDiagnostic, Result};

const LIBRARY: &str = include_str!("fixtures/macro_islands/lib.rs.txt");

fn project() -> Result<(tempfile::TempDir, CrateIr)> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='islands'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = database
        .project(
            &CrateTarget::new("islands", root.path()),
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
        .ok_or_else(|| miette::miette!("no node `{name}`"))
}

fn callee_paths(graph: &CrateIr, name: &str) -> Result<Vec<String>> {
    let mut paths: Vec<String> = graph
        .neighbors(
            node(graph, name)?,
            EdgeKind::Calls,
            petgraph::Direction::Outgoing,
        )
        .into_iter()
        .filter_map(|(_, id)| graph.node(id))
        .filter_map(|node| node.attr("qualified_path")?.as_str().map(str::to_owned))
        .collect();
    paths.sort();
    Ok(paths)
}

#[test]
fn a_function_in_an_unexpandable_macro_is_gated_by_its_attributes() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    let island = graph
        .macro_island(node(&graph, "verify_checker")?)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("not marked as read from a macro"))?;
    assert_eq!(island.gates(), &["kani".to_owned()]);
    Ok(())
}

#[test]
fn a_function_with_no_gating_attribute_has_no_gates() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    let island = graph
        .macro_island(node(&graph, "ordinary_in_macro")?)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("not marked as read from a macro"))?;
    assert!(island.gates().is_empty());
    Ok(())
}

#[test]
fn calls_in_the_island_are_resolved_at_the_invocation() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    assert_eq!(
        callee_paths(&graph, "verify_checker")?,
        ["Ensures::ensures", "helper"]
    );
    assert_eq!(callee_paths(&graph, "ordinary_in_macro")?, ["helper"]);
    Ok(())
}

#[test]
fn inherent_and_trait_qualified_paths_resolve() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    assert_eq!(
        callee_paths(&graph, "qualified_paths")?,
        ["Checker::inherent", "Ensures::ensures"]
    );
    Ok(())
}

#[test]
fn a_real_function_is_not_marked_as_read_from_a_macro() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    assert_eq!(
        graph
            .macro_island(node(&graph, "helper")?)
            .into_diagnostic()?,
        None
    );
    Ok(())
}
