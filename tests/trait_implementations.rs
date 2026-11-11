//! A function that implements a trait method says so, so that a call through the
//! trait can be followed to it; a trait outside the workspace says that instead.

use cordial::{
    CfgPlanning, CrateIr, FunctionLocation, IrView, NodeId, RunAll, SemanticCargoOptions,
    SourceDatabase, TraitImplementation,
};
use miette::{IntoDiagnostic, Result};

const WORKSPACE: &str = include_str!("fixtures/trait_implementations/workspace.toml.txt");
const PROVIDER_MANIFEST: &str = include_str!("fixtures/trait_implementations/provider.toml.txt");
const PROVIDER_LIB: &str = include_str!("fixtures/trait_implementations/provider_lib.rs.txt");
const CONSUMER_MANIFEST: &str = include_str!("fixtures/trait_implementations/consumer.toml.txt");
const CONSUMER_LIB: &str = include_str!("fixtures/trait_implementations/consumer_lib.rs.txt");

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

/// The function nodes with this name whose qualified path mentions `within`.
fn function(graph: &CrateIr, name: &str, within: &str) -> Result<NodeId> {
    let mut found = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .filter(|node| node.weight().name().as_deref() == Some(name))
        .filter(|node| {
            node.attr("qualified_path")
                .and_then(|path| path.as_str())
                .is_some_and(|path| path.contains(within))
        })
        .map(|node| node.id());
    match (found.next(), found.next()) {
        (Some(id), None) => Ok(id),
        _ => Err(miette::miette!("expected exactly one `{within}::{name}`")),
    }
}

fn implementation(
    graph: &CrateIr,
    name: &str,
    within: &str,
) -> Result<Option<TraitImplementation>> {
    graph
        .trait_implementation(function(graph, name, within)?)
        .into_diagnostic()
}

/// Where the trait method `trait_name::name` is declared.
fn declaration(graph: &CrateIr, trait_name: &str, name: &str) -> Result<FunctionLocation> {
    let path = format!("{trait_name}::{name}");
    let node = graph
        .nodes_matching(&cordial::BasicQuery::items())
        .into_iter()
        .find(|node| {
            node.attr("qualified_path").and_then(|value| value.as_str()) == Some(path.as_str())
        })
        .ok_or_else(|| miette::miette!("no declaration `{path}`"))?;
    graph
        .function_location(node.id())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("`{path}` has no location"))
}

#[test]
fn an_implementation_of_a_trait_in_another_workspace_crate_names_its_declaration() -> Result<()> {
    cordial::init_tracing();
    let projected = project()?;
    let declared = implementation(&projected.consumer, "check", "Local")?
        .ok_or_else(|| miette::miette!("`check` is an implementation"))?;
    assert_eq!(
        declared.declaration().as_ref(),
        Some(&declaration(&projected.provider, "Rule", "check")?)
    );
    Ok(())
}

#[test]
fn an_implementation_of_a_trait_in_the_same_crate_names_its_declaration() -> Result<()> {
    cordial::init_tracing();
    let projected = project()?;
    let declared = implementation(&projected.consumer, "own", "Local")?
        .ok_or_else(|| miette::miette!("`own` is an implementation"))?;
    assert_eq!(
        declared.declaration().as_ref(),
        Some(&declaration(&projected.consumer, "Own", "own")?)
    );
    Ok(())
}

#[test]
fn an_implementation_of_a_foreign_trait_says_its_declaration_is_foreign() -> Result<()> {
    cordial::init_tracing();
    let projected = project()?;
    let declared = implementation(&projected.consumer, "fmt", "Local")?
        .ok_or_else(|| miette::miette!("`fmt` is an implementation"))?;
    assert_eq!(declared.declaration(), &None);
    Ok(())
}

#[test]
fn an_inherent_method_is_not_an_implementation() -> Result<()> {
    cordial::init_tracing();
    let projected = project()?;
    assert_eq!(
        implementation(&projected.consumer, "inherent", "Local")?,
        None
    );
    Ok(())
}

#[test]
fn an_implementation_that_exists_only_under_a_cfg_context_is_recorded() -> Result<()> {
    cordial::init_tracing();
    let projected = project()?;
    let declared = implementation(&projected.consumer, "own", "Gated")?
        .ok_or_else(|| miette::miette!("the gated `own` is an implementation"))?;
    assert_eq!(
        declared.declaration().as_ref(),
        Some(&declaration(&projected.consumer, "Own", "own")?)
    );
    Ok(())
}
