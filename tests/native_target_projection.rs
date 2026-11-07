//! Target-driven projection retains Cargo identities and complete policy inputs.
#![cfg(all(feature = "allows", feature = "cfg_hygiene", feature = "cfg_scatter"))]
// Slow (over 60s): compiled only without the default `quick` feature; run with `just test-slow`.
#![cfg(not(feature = "quick"))]

use cordial::{CrateKind, IrView, QueryBuilder, RunAll, SourceDatabase, SourceSpan};
use miette::{IntoDiagnostic, Result};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn target_selection_preserves_entries_inactive_sources_and_test_policies() -> Result<()> {
    cordial::init_cli_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    for dir in ["src", "library", "cli", "tests/fixtures"] {
        std::fs::create_dir_all(root.path().join(dir)).into_diagnostic()?;
    }
    std::fs::write(
        root.path().join("Cargo.toml"),
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
        std::fs::write(root.path().join(path), text).into_diagnostic()?;
    }
    let targets = cordial::discover_crate_targets(root.path(), &RunAll).into_diagnostic()?;
    assert_eq!(targets.len(), 1);
    let target = &targets[0];
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let graph = database.project_target(target).into_diagnostic()?;
    let stale = target.clone().with_manifest_snapshot(Some(
        serde_json::from_value(serde_json::json!({
            "path": root.path().join("Cargo.toml"),
            "digest": ([0_u8; 32]),
        }))
        .into_diagnostic()?,
    ));
    assert!(
        database.project_target(&stale).is_err(),
        "target metadata must belong to the loaded Cargo context"
    );
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
                .strip_prefix(root.path())
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
    assert_eq!(package.root(), root.path());
    assert_eq!(
        package.source_entries(),
        &[
            root.path().join("cli/entry.rs"),
            root.path().join("library/api.rs")
        ]
    );
    #[cfg(feature = "derives")]
    {
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
            .map(|path| root.path().join(path))
        );
        #[cfg(feature = "tracing")]
        for (file, function) in [
            ("library/child.rs", "child::library_child"),
            ("cli/command.rs", "command::command"),
        ] {
            let source = workspace
                .sources()
                .iter()
                .find(|source| source.snapshot().path() == &root.path().join(file))
                .ok_or_else(|| miette::miette!("missing custom module source"))?;
            assert!(
                source
                    .proof_calls()
                    .iter()
                    .any(|call| call.qualified_name() == function)
            );
        }
    }
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
                .is_some_and(|span| !span.file().starts_with(root.path().join("tests"))))
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
    database
        .change_file(
            &root.path().join("tests/integration.rs"),
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
        std::fs::read_to_string(root.path().join("tests/integration.rs")).into_diagnostic()?,
        "#[allow(dead_code)] fn integration_only() {}\n"
    );
    Ok(())
}
