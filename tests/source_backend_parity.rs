//! Migration checks compare complete source payloads from independent sessions.

use std::path::Path;

use cordial::{
    CrateIr, CrateTarget, IrView, LoadContext, Loader, QueryBuilder, SessionBuilder,
    SourceDatabase, SourceLoadView, SourceLoader,
};
use miette::{IntoDiagnostic, Result};

const SOURCE: &str = include_str!("fixtures/semantic_backend/syntax.rs.txt");

#[tracing::instrument(level = "debug", skip(root), err(level = "warn"))]
fn baseline(root: &Path) -> Result<CrateIr> {
    let session = SessionBuilder::new(root).build();
    let target = CrateTarget::new("source_parity", root);
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let view = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("unexpected loader"))?;
    let mut graph = CrateIr::new("source_parity");
    view.populate_ir(&mut graph).into_diagnostic()?;
    Ok(graph)
}

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
fn module_scope(graph: &CrateIr) -> Result<cordial::SourceModuleScope> {
    let nodes = graph.nodes_matching(
        &QueryBuilder::new()
            .has_attr(cordial::ATTR_SOURCE_MODULE_SCOPE)
            .build(),
    );
    let node = nodes
        .into_iter()
        .next()
        .ok_or_else(|| miette::miette!("source module missing"))?;
    graph
        .source_module_scope(node.id())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("source module facts missing"))
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn database_projects_the_existing_source_contract_and_inactive_syntax() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"source_parity\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n").into_diagnostic()?;
    let path = root.path().join("src/lib.rs");
    std::fs::write(&path, SOURCE).into_diagnostic()?;
    // Capture and drop the compatibility source session before loading the
    // selected backend. Production never runs both parsers over one input.
    let legacy = baseline(root.path())?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let native = database
        .project("source_parity", std::slice::from_ref(&path))
        .into_diagnostic()?;
    let legacy_scope = module_scope(&legacy)?;
    let native_scope = module_scope(&native)?;
    assert_eq!(legacy_scope.identifiers(), native_scope.identifiers());
    assert_eq!(legacy_scope.uses(), native_scope.uses());
    assert_eq!(legacy_scope, native_scope);
    let query = QueryBuilder::new()
        .has_attr(cordial::ATTR_FUNCTION_SOURCE_SYNTAX)
        .build();
    let legacy_functions = legacy.nodes_matching(&query);
    assert_eq!(legacy_functions.len(), native.nodes_matching(&query).len());
    for node in legacy_functions {
        let name = node
            .weight()
            .name()
            .as_deref()
            .ok_or_else(|| miette::miette!("unnamed function"))?;
        let native_node = native
            .node_by_path(name)
            .ok_or_else(|| miette::miette!("missing native function {name}"))?;
        assert_eq!(
            legacy.function_source_syntax(node.id()).into_diagnostic()?,
            native
                .function_source_syntax(native_node)
                .into_diagnostic()?,
            "complete syntax payload for {name}"
        );
    }
    let inactive = native
        .node_by_path("inactive")
        .ok_or_else(|| miette::miette!("inactive syntax missing"))?;
    assert!(
        native
            .function_source_syntax(inactive)
            .into_diagnostic()?
            .is_some()
    );
    assert!(
        native
            .semantic_function(inactive)
            .into_diagnostic()?
            .is_none(),
        "inactive syntax cannot invent a semantic definition"
    );
    for name in ["inline", "inline::Widget", "inline::nested"] {
        assert!(
            native.node_by_path(name).is_some(),
            "missing nested source item {name}"
        );
    }
    database
        .change_file(&path, "fn broken( {".into())
        .into_diagnostic()?;
    assert!(
        database
            .project("source_parity", std::slice::from_ref(&path))
            .is_err(),
        "parser recovery cannot authorize incomplete source facts"
    );
    database
        .change_file(&path, SOURCE.into())
        .into_diagnostic()?;
    let restored = database
        .project("source_parity", &[path])
        .into_diagnostic()?;
    assert_eq!(module_scope(&native)?, module_scope(&restored)?);
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/lib.rs")).into_diagnostic()?,
        SOURCE
    );
    Ok(())
}
