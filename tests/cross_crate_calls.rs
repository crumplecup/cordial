//! A call into another workspace crate is recorded where it is made, by the
//! callee's source location, in the ordinary build and in a cfg context.

use cordial::{
    CfgPlanning, CrateIr, ExternalCall, FunctionLocation, IrView, NodeId, RunAll,
    SemanticCargoOptions, SemanticContextKey, SourceDatabase,
};
use miette::{IntoDiagnostic, Result};

const WORKSPACE: &str = include_str!("fixtures/cross_crate_calls/workspace.toml.txt");
const PROVIDER_MANIFEST: &str = include_str!("fixtures/cross_crate_calls/provider.toml.txt");
const PROVIDER_LIB: &str = include_str!("fixtures/cross_crate_calls/provider_lib.rs.txt");
const CONSUMER_MANIFEST: &str = include_str!("fixtures/cross_crate_calls/consumer.toml.txt");
const CONSUMER_LIB: &str = include_str!("fixtures/cross_crate_calls/consumer_lib.rs.txt");

/// Both crates of the specimen workspace, projected from one database.
struct Projected {
    _root: tempfile::TempDir,
    provider: CrateIr,
    consumer: CrateIr,
}

fn project() -> Result<Projected> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for (dir, manifest, lib) in [
        ("provider", PROVIDER_MANIFEST, PROVIDER_LIB),
        ("consumer", CONSUMER_MANIFEST, CONSUMER_LIB),
    ] {
        let crate_root = root.path().join(dir);
        std::fs::create_dir_all(crate_root.join("src")).into_diagnostic()?;
        std::fs::write(crate_root.join("Cargo.toml"), manifest).into_diagnostic()?;
        std::fs::write(crate_root.join("src/lib.rs"), lib).into_diagnostic()?;
    }
    std::fs::write(root.path().join("Cargo.toml"), WORKSPACE).into_diagnostic()?;
    let options = SemanticCargoOptions::default().with_cfg_planning(CfgPlanning::Automatic);
    let mut database = SourceDatabase::load_with(root.path(), &options).into_diagnostic()?;
    let targets = cordial::discover_crate_targets(root.path(), &RunAll).into_diagnostic()?;
    let mut project = |name: &str| -> Result<CrateIr> {
        let target = targets
            .iter()
            .find(|target| target.crate_name() == name)
            .ok_or_else(|| miette::miette!("no target {name}"))?;
        database.project_target(target).into_diagnostic()
    };
    let provider = project("provider")?;
    let consumer = project("consumer")?;
    Ok(Projected {
        _root: root,
        provider,
        consumer,
    })
}

fn function(graph: &CrateIr, name: &str) -> Result<NodeId> {
    let mut found = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .filter(|node| node.weight().name().as_deref() == Some(name))
        .map(|node| node.id());
    match (found.next(), found.next()) {
        (Some(id), None) => Ok(id),
        _ => Err(miette::miette!("expected exactly one `{name}`")),
    }
}

fn location(graph: &CrateIr, name: &str) -> Result<FunctionLocation> {
    graph
        .function_location(function(graph, name)?)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("`{name}` has no recorded location"))
}

fn calls(graph: &CrateIr, name: &str) -> Result<Vec<ExternalCall>> {
    graph
        .external_calls(function(graph, name)?)
        .into_diagnostic()
}

#[test]
fn every_function_records_where_it_starts() -> Result<()> {
    cordial::init_tracing();
    let projected = project()?;
    let shared = location(&projected.provider, "shared")?;
    assert!(shared.file().ends_with("provider/src/lib.rs"), "{shared:?}");
    assert_eq!(*shared.offset(), 0);
    let ensures = location(&projected.provider, "ensures")?;
    assert_eq!(ensures.file(), shared.file());
    assert_ne!(ensures.offset(), shared.offset());
    Ok(())
}

#[test]
fn an_ordinary_call_into_another_crate_names_the_callee_by_location() -> Result<()> {
    cordial::init_tracing();
    let projected = project()?;
    let ordinary: Vec<_> = calls(&projected.consumer, "ordinary")?
        .into_iter()
        .filter(|call| call.context().is_none())
        .collect();
    assert_eq!(ordinary.len(), 1, "{ordinary:?}");
    assert_eq!(
        ordinary[0].callee(),
        &location(&projected.provider, "shared")?
    );
    Ok(())
}

#[test]
fn a_gated_call_into_another_crate_names_its_context() -> Result<()> {
    cordial::init_tracing();
    let projected = project()?;
    let made = calls(&projected.consumer, "harness")?;
    assert_eq!(made.len(), 1, "{made:?}");
    assert_eq!(made[0].callee(), &location(&projected.provider, "ensures")?);
    assert_eq!(
        made[0].context(),
        &Some(SemanticContextKey::enabling(["kani"]))
    );
    Ok(())
}

#[test]
fn a_call_that_stays_in_its_crate_is_not_external() -> Result<()> {
    cordial::init_tracing();
    let projected = project()?;
    assert!(calls(&projected.provider, "shared")?.is_empty());
    Ok(())
}
