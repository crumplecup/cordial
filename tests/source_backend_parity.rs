//! Migration checks compare complete source payloads from independent sessions.

use std::path::Path;

use cordial::{
    CrateIr, CrateTarget, IrView, LoadContext, Loader, QueryBuilder, SessionBuilder,
    SourceDatabase, SourceLoadView, SourceLoader,
};
use miette::{IntoDiagnostic, Result};

#[path = "support/source_attribute_projection.rs"]
mod source_attribute_projection;
#[cfg(feature = "tracing")]
#[path = "support/tracing_source_projection.rs"]
mod tracing_source_projection;

#[cfg(all(feature = "allows", feature = "cfg_hygiene", feature = "cfg_scatter"))]
#[path = "support/source_policy_projection.rs"]
mod source_policy_projection;

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

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
fn file_attributes(graph: &CrateIr) -> Result<cordial::SourceFileAttributes> {
    let node = graph
        .nodes_matching(
            &QueryBuilder::new()
                .has_attr(cordial::ATTR_SOURCE_FILE_ATTRIBUTES)
                .build(),
        )
        .into_iter()
        .next()
        .ok_or_else(|| miette::miette!("source attributes missing"))?;
    graph
        .source_file_attributes(node.id())
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("source attribute payload missing"))
}

#[tracing::instrument(level = "debug", skip(legacy, native), err(level = "warn"))]
fn assert_attribute_contract(legacy: &CrateIr, native: &CrateIr) -> Result<()> {
    let expected = file_attributes(legacy)?;
    let actual = file_attributes(native)?;
    assert_eq!(expected, actual, "complete file attribute inventory");
    assert!(actual.lints().forbid_unsafe_code());
    assert!(actual.lints().missing_docs_at_least_warn());
    assert!(
        actual
            .items()
            .iter()
            .any(|item| item.path() == "AttributeOwner"
                && item
                    .attributes()
                    .iter()
                    .any(|attr| attr.meta() == "derive(Debug)")),
        "top-level type attributes must reach the shared inventory"
    );
    assert!(
        !actual
            .items()
            .iter()
            .any(|item| item.path().starts_with("excluded_attributes::"))
    );
    for path in [
        "attributed",
        "<AttributeOwner<T, N> as AttributeTrait>::method",
        "AttributeOwner<(&mut u32, [u8; 16]), 4>::tuple_method",
        "inactive_attributes::nested",
    ] {
        assert!(
            actual.items().iter().any(|item| item.path() == path
                && item.attributes().iter().any(|attr| attr.instrument())),
            "missing attributed owner {path}"
        );
    }
    assert!(
        actual
            .items()
            .iter()
            .flat_map(|item| item.attributes())
            .any(|attr| attr.meta().contains("\"keep : : spaces\"")),
        "literal contents must survive token rendering"
    );
    let mut damaged = native.clone();
    let node = damaged.nodes_matching(
        &QueryBuilder::new()
            .has_attr(cordial::ATTR_SOURCE_FILE_ATTRIBUTES)
            .build(),
    )[0]
    .id();
    damaged
        .set_attr(
            node,
            cordial::ATTR_SOURCE_FILE_ATTRIBUTES,
            serde_json::json!({"items": []}),
        )
        .into_diagnostic()?;
    assert!(
        damaged.source_file_attributes(node).is_err(),
        "corrupt facts cannot become an empty inventory"
    );
    Ok(())
}

#[tracing::instrument(level = "debug", skip(database), err(level = "warn"))]
fn assert_virtual_attribute_policies(database: &mut SourceDatabase, path: &Path) -> Result<()> {
    for (source, unsafe_code, missing_docs) in [
        (
            "#![forbid(dead_code, reason = \"unsafe_code, missing_docs\")]\n",
            false,
            false,
        ),
        (
            "#![cfg_attr(any(), forbid(unsafe_code), deny(missing_docs))]\n",
            true,
            true,
        ),
        (
            "#![warn(nested(unsafe_code, missing_docs))]\n",
            false,
            false,
        ),
        ("#![allow(missing_docs)]\n", false, false),
        (
            "#![cfg_attr(any(), cfg_attr(any(), forbid(unsafe_code)))]\n",
            false,
            false,
        ),
        ("#![forbid(module::unsafe_code)]\n", true, false),
        ("//! An itemless file.\n", false, false),
    ] {
        database
            .change_file(path, source.into())
            .into_diagnostic()?;
        let graph = database
            .project(
                &cordial::CrateTarget::new(
                    "source_parity",
                    path.parent()
                        .and_then(Path::parent)
                        .ok_or_else(|| miette::miette!("source root missing"))?,
                ),
                &[path.to_path_buf()],
            )
            .into_diagnostic()?;
        let facts = file_attributes(&graph)?;
        assert_eq!(facts.lints().forbid_unsafe_code(), unsafe_code, "{source}");
        assert_eq!(
            facts.lints().missing_docs_at_least_warn(),
            missing_docs,
            "{source}"
        );
        assert_eq!(
            facts.insert_at(),
            source.lines().count(),
            "itemless insertion point"
        );
        assert!(facts.items().is_empty());
    }
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn database_projects_the_existing_source_contract_and_inactive_syntax() -> Result<()> {
    cordial::init_cli_tracing();
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
        .project(
            &cordial::CrateTarget::new("source_parity", root.path()),
            std::slice::from_ref(&path),
        )
        .into_diagnostic()?;
    let legacy_scope = module_scope(&legacy)?;
    let native_scope = module_scope(&native)?;
    assert_eq!(legacy_scope.identifiers(), native_scope.identifiers());
    assert_eq!(legacy_scope.uses(), native_scope.uses());
    assert_eq!(legacy_scope, native_scope);
    assert_attribute_contract(&legacy, &native)?;
    #[cfg(any(
        feature = "allows",
        feature = "cfg_hygiene",
        feature = "cfg_scatter",
        feature = "glob_imports",
        feature = "inline_tests"
    ))]
    assert_eq!(
        legacy.source_policy_inventory().into_diagnostic()?,
        native.source_policy_inventory().into_diagnostic()?,
        "complete source policy inventory"
    );
    #[cfg(feature = "tracing")]
    tracing_source_projection::assert_contract(&legacy, &native)?;
    source_attribute_projection::assert_graph_only_attributes(&legacy, root.path())?;
    source_attribute_projection::assert_graph_only_attributes(&native, root.path())?;
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
    assert_virtual_attribute_policies(&mut database, &path)?;
    database
        .change_file(&path, "fn broken( {".into())
        .into_diagnostic()?;
    assert!(
        database
            .project(
                &cordial::CrateTarget::new("source_parity", root.path()),
                std::slice::from_ref(&path)
            )
            .is_err(),
        "parser recovery cannot authorize incomplete source facts"
    );
    database
        .change_file(&path, SOURCE.into())
        .into_diagnostic()?;
    let restored = database
        .project(
            &cordial::CrateTarget::new("source_parity", root.path()),
            &[path],
        )
        .into_diagnostic()?;
    assert_eq!(module_scope(&native)?, module_scope(&restored)?);
    assert_eq!(file_attributes(&native)?, file_attributes(&restored)?);
    assert_eq!(
        std::fs::read_to_string(root.path().join("src/lib.rs")).into_diagnostic()?,
        SOURCE
    );
    Ok(())
}

#[test]
#[cfg(all(feature = "allows", feature = "cfg_hygiene", feature = "cfg_scatter"))]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn file_policies_use_shared_evidence_and_track_virtual_revisions() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .finish(),
    );
    source_policy_projection::verify()
}
