//! Target-driven projection retains Cargo identities and complete policy inputs.
#![cfg(all(feature = "allows", feature = "cfg_hygiene", feature = "cfg_scatter"))]

use cordial::{
    CrateIr, CrateKind, CrateTarget, IrView, QueryBuilder, RunAll, SourceDatabase, SourceSpan,
};
use miette::{IntoDiagnostic, Result};

/// A package with a custom `[lib]` and `[[bin]]` path, a detached `src` file behind an inactive
/// `cfg`, an integration test that allows a lint, and a malformed fixture that must be ignored.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn write_target_selection_workspace(root: &std::path::Path) -> Result<()> {
    for dir in ["src", "library", "cli", "tests/fixtures"] {
        std::fs::create_dir_all(root.join(dir)).into_diagnostic()?;
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'target_selection'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n[lib]\npath = 'library/api.rs'\n[[bin]]\nname = 'custom_cli'\npath = 'cli/entry.rs'\n",
    ).into_diagnostic()?;
    for (path, text) in [
        ("library/api.rs", "mod child; pub fn library_entry() {}\n"),
        ("library/child.rs", "pub fn library_child() {}\n"),
        ("cli/entry.rs", "mod command; fn main() {}\n"),
        ("cli/command.rs", "pub fn command() {}\n"),
        ("src/detached.rs", "#[cfg(any())] fn inactive() {}\n"),
        (
            "tests/integration.rs",
            "#[allow(dead_code)] fn integration_only() {}\n",
        ),
        ("tests/fixtures/ignored.rs", "fn malformed( {"),
    ] {
        std::fs::write(root.join(path), text).into_diagnostic()?;
    }
    Ok(())
}

/// Target metadata is tied to the Cargo context that was loaded: a stale manifest identity fails.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_stale_manifest_is_rejected(
    database: &mut SourceDatabase,
    target: &CrateTarget,
    root: &std::path::Path,
) -> Result<()> {
    let stale = target.clone().with_manifest_snapshot(Some(
        serde_json::from_value(serde_json::json!({
            "path": root.join("Cargo.toml"),
            "digest": ([0_u8; 32]),
        }))
        .into_diagnostic()?,
    ));
    assert!(
        database.project_target(&stale).is_err(),
        "target metadata must belong to the loaded Cargo context"
    );
    Ok(())
}

/// The projected graph keeps the Cargo target kinds, manifest identity and binary entries.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_target_identity(graph: &CrateIr, target: &CrateTarget) -> Result<()> {
    assert_eq!(graph.crate_kinds().into_diagnostic()?, *target.kinds());
    assert!(
        graph
            .crate_kinds()
            .into_diagnostic()?
            .contains(&CrateKind::Lib)
    );
    assert!(
        graph
            .crate_kinds()
            .into_diagnostic()?
            .contains(&CrateKind::Bin)
    );
    let root_node = graph
        .node(graph.root())
        .ok_or_else(|| miette::miette!("missing root"))?;
    assert_eq!(
        root_node.attr(cordial::ATTR_CARGO_MANIFEST_SNAPSHOT),
        Some(
            &serde_json::to_value(
                target
                    .manifest_snapshot()
                    .as_ref()
                    .ok_or_else(|| miette::miette!("missing manifest identity"))?
            )
            .into_diagnostic()?
        )
    );
    assert_eq!(
        root_node.attr(cordial::ATTR_BINARY_ENTRY_PATHS),
        Some(&serde_json::to_value(target.binary_entries()).into_diagnostic()?)
    );
    Ok(())
}

/// The semantic snapshot lists every file of the target, detached and test sources included, and
/// the package names its custom entry points.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_snapshot_sources(graph: &CrateIr, root: &std::path::Path) -> Result<()> {
    let snapshot = graph
        .semantic_snapshot()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing snapshot"))?;
    let mut paths: Vec<_> = snapshot
        .sources()
        .iter()
        .map(|source| {
            source
                .path()
                .strip_prefix(root)
                .map(std::path::Path::to_path_buf)
                .into_diagnostic()
        })
        .collect::<Result<_>>()?;
    paths.sort();
    assert_eq!(
        paths,
        [
            "cli/command.rs",
            "cli/entry.rs",
            "library/api.rs",
            "library/child.rs",
            "src/detached.rs",
            "tests/integration.rs"
        ]
        .map(std::path::PathBuf::from)
    );
    let package = snapshot
        .workspace()
        .packages()
        .iter()
        .find(|package| package.name() == "target_selection")
        .ok_or_else(|| miette::miette!("missing package identity"))?;
    assert_eq!(package.root(), root);
    assert_eq!(
        package.source_entries(),
        &[root.join("cli/entry.rs"), root.join("library/api.rs")]
    );
    Ok(())
}

/// Path inclusion covers custom module files, and each proves its own calls.
#[cfg(feature = "derives")]
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_path_inclusions(database: &mut SourceDatabase, root: &std::path::Path) -> Result<()> {
    let workspace = database.project_path_inclusions().into_diagnostic()?;
    let mut paths: Vec<_> = workspace
        .sources()
        .iter()
        .map(|source| source.snapshot().path().clone())
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "cli/command.rs",
            "cli/entry.rs",
            "library/api.rs",
            "library/child.rs",
            "src/detached.rs"
        ]
        .map(|path| root.join(path))
    );
    #[cfg(feature = "tracing")]
    for (file, function) in [
        ("library/child.rs", "child::library_child"),
        ("cli/command.rs", "command::command"),
    ] {
        let source = workspace
            .sources()
            .iter()
            .find(|source| source.snapshot().path() == &root.join(file))
            .ok_or_else(|| miette::miette!("missing custom module source"))?;
        assert!(
            source
                .proof_calls()
                .iter()
                .any(|call| call.qualified_name() == function)
        );
    }
    Ok(())
}

/// The integration test's `allow` is in the policy inventory, but its functions are not module
/// scopes of the library, and a function behind an inactive `cfg` has no semantic facts.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_policy_and_inactive_code(graph: &CrateIr, root: &std::path::Path) -> Result<()> {
    let policy = graph
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing policy"))?;
    assert!(
        policy
            .allows()
            .iter()
            .any(|site| site.file() == std::path::Path::new("tests/integration.rs"))
    );
    assert!(
        graph
            .nodes_matching(
                &QueryBuilder::new()
                    .has_attr(cordial::ATTR_SOURCE_MODULE_SCOPE)
                    .build()
            )
            .iter()
            .all(|node| node
                .weight()
                .span()
                .as_ref()
                .is_some_and(|span| !span.file().starts_with(root.join("tests"))))
    );
    let inactive = graph
        .nodes_matching(
            &QueryBuilder::new()
                .has_attr(cordial::ATTR_FUNCTION_SOURCE_SYNTAX)
                .build(),
        )
        .into_iter()
        .find(|node| node.weight().name().as_deref() == Some("inactive"))
        .ok_or_else(|| miette::miette!("missing inactive function"))?;
    assert!(
        graph
            .semantic_function(inactive.id())
            .into_diagnostic()?
            .is_none()
    );
    Ok(())
}

/// Editing the integration test in the database drops its `allow` from a new projection while the
/// file on disk is untouched.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
fn assert_edit_revises_policy_without_touching_disk(
    database: &mut SourceDatabase,
    target: &CrateTarget,
    root: &std::path::Path,
) -> Result<()> {
    database
        .change_file(
            &root.join("tests/integration.rs"),
            "fn integration_only() {}\n".into(),
        )
        .into_diagnostic()?;
    let revised = database.project_target(target).into_diagnostic()?;
    assert!(
        revised
            .source_policy_inventory()
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("missing revised policy"))?
            .allows()
            .is_empty()
    );
    assert_eq!(
        std::fs::read_to_string(root.join("tests/integration.rs")).into_diagnostic()?,
        "#[allow(dead_code)] fn integration_only() {}\n"
    );
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn target_selection_preserves_entries_inactive_sources_and_test_policies() -> Result<()> {
    cordial::init_cli_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    write_target_selection_workspace(root.path())?;
    let targets = cordial::discover_crate_targets(root.path(), &RunAll).into_diagnostic()?;
    assert_eq!(targets.len(), 1);
    let target = &targets[0];
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = database.project_target(target).into_diagnostic()?;
    assert_stale_manifest_is_rejected(&mut database, target, root.path())?;
    assert_target_identity(&graph, target)?;
    assert_snapshot_sources(&graph, root.path())?;
    #[cfg(feature = "derives")]
    assert_path_inclusions(&mut database, root.path())?;
    assert_policy_and_inactive_code(&graph, root.path())?;
    assert_edit_revises_policy_without_touching_disk(&mut database, target, root.path())
}
