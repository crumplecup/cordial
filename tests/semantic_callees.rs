//! A call keeps the definition it resolved to: its crate and its canonical path,
//! including the trait or type it belongs to, however it was written.

use cordial::{CrateIr, CrateTarget, IrView, SourceDatabase};
use miette::{IntoDiagnostic, Result};

const LIBRARY: &str = include_str!("fixtures/semantic_callees/lib.rs.txt");

fn project() -> Result<(tempfile::TempDir, CrateIr)> {
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='callees'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.path().join("src/lib.rs"), LIBRARY).into_diagnostic()?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = database
        .project(
            &CrateTarget::new("callees", root.path()),
            &[root.path().join("src/lib.rs")],
        )
        .into_diagnostic()?;
    Ok((root, graph))
}

/// The canonical paths of what `function` calls, in order.
fn callees(graph: &CrateIr, function: &str) -> Result<Vec<String>> {
    let node = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .find(|node| node.weight().name().as_deref() == Some(function))
        .ok_or_else(|| miette::miette!("no function `{function}`"))?;
    let facts = graph
        .semantic_function(node.id())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("`{function}` has no semantic facts"))?;
    Ok(facts
        .calls()
        .iter()
        .filter_map(|call| call.callee().as_ref().map(|callee| callee.path().clone()))
        .collect())
}

#[test]
fn a_free_function_an_inherent_method_and_a_trait_method_are_told_apart() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    assert_eq!(
        callees(&graph, "caller")?,
        [
            "callees::util::free",
            "callees::Builder::finish",
            "callees::Install::install",
        ]
    );
    Ok(())
}

#[test]
fn a_call_inside_a_closure_is_a_call_of_the_function() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    assert_eq!(callees(&graph, "closure_caller")?, ["callees::util::free"]);
    Ok(())
}

#[test]
fn the_callee_knows_its_crate_and_its_own_name() -> Result<()> {
    cordial::init_tracing();
    let (_root, graph) = project()?;
    let node = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .find(|node| node.weight().name().as_deref() == Some("caller"))
        .ok_or_else(|| miette::miette!("no caller"))?;
    let facts = graph
        .semantic_function(node.id())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no facts"))?;
    let first = facts.calls()[0]
        .callee()
        .clone()
        .ok_or_else(|| miette::miette!("no callee"))?;
    assert!(first.is(&["callees"], &["free"]));
    assert!(!first.is(&["other"], &["free"]));
    assert!(!first.is(&["callees"], &["finish"]));
    Ok(())
}
