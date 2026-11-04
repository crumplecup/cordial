//! Module hierarchy evidence, graph-only visibility policy and revision recovery.
#![cfg(feature = "visibility")]
use cordial::{
    CrateIr, CrateTarget, CrateViewMut, IrView, LoadContext, Loader, QueryBuilder, SessionBuilder,
    SourceDatabase, SourceEnrichView, SourceEnricher, SourceLoadView, SourceLoader,
    VisibilityInventoryEnricher, WorkspaceIr,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const SOURCE: &str = include_str!("fixtures/semantic_backend/visibility.rs.txt");

#[tracing::instrument(level = "debug", skip(target), err(level = "warn"))]
fn baseline(root: &Path, target: &CrateTarget) -> Result<CrateIr> {
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("unexpected loader"))?;
    let mut graph = CrateIr::new("visibility_parity");
    source.populate_ir(&mut graph).into_diagnostic()?;
    Ok(graph)
}

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
fn materialize(graph: &CrateIr, root: &Path) -> Result<Vec<serde_json::Value>> {
    let empty = tempfile::tempdir().into_diagnostic()?;
    let target = CrateTarget::new("empty", empty.path());
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("unexpected loader"))?;
    assert!(source.files().is_empty());
    let mut workspace = WorkspaceIr::default();
    workspace.insert_crate(graph.clone());
    let mut view = CrateViewMut::new(&mut workspace, graph.crate_name().into());
    VisibilityInventoryEnricher
        .enrich_source(SourceEnrichView::new(&mut view, source, &session))
        .into_diagnostic()?;
    let mut rows = Vec::new();
    for node in view.nodes_matching(&QueryBuilder::new().has_attr("visibility_rule_id").build()) {
        let attrs: std::collections::BTreeMap<_, _> =
            node.weight().attrs().iter().cloned().collect();
        rows.push(serde_json::json!({"attrs":attrs, "span":node.weight().span()}));
    }
    rows.sort_by_key(serde_json::Value::to_string);
    Ok(rows)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn visibility_tree_and_policy_survive_backend_changes_and_virtual_edits() -> Result<()> {
    cordial::init_tracing();
    let _subscriber = tracing::subscriber::set_default(tracing_subscriber::registry());
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src/external")).into_diagnostic()?;
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"visibility_parity\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n").into_diagnostic()?;
    let path = root.path().join("src/lib.rs");
    let external = root.path().join("src/external.rs");
    let child = root.path().join("src/external/child/mod.rs");
    std::fs::create_dir_all(
        child
            .parent()
            .ok_or_else(|| miette::miette!("child parent"))?,
    )
    .into_diagnostic()?;
    std::fs::write(&path, SOURCE).into_diagnostic()?;
    std::fs::write(&external, "pub struct Leaf;\npub mod child;\n").into_diagnostic()?;
    std::fs::write(&child, "pub fn child() {}\n").into_diagnostic()?;
    std::fs::write(root.path().join("support.rs"), "pub fn custom() {}\n").into_diagnostic()?;
    let target = CrateTarget::new("visibility_parity", root.path());
    let legacy = baseline(root.path(), &target)?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    // The module collector follows the target tree using the same database,
    // including external bodies outside the generic src scan selection.
    let native = database
        .project(&target, &[path.clone()])
        .into_diagnostic()?;
    let expected = legacy
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing compatibility facts"))?;
    let actual = native
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing native facts"))?;
    assert_eq!(
        expected.visibility(),
        actual.visibility(),
        "complete hierarchy and snapshots"
    );
    let rows = materialize(&native, root.path())?;
    assert_eq!(
        materialize(&legacy, root.path())?,
        rows,
        "all attributes and spans"
    );
    for rule in [
        "VIS-CRATE-FLAT-001",
        "VIS-MOD-THIN-001",
        "VIS-MOD-MISMATCH-001",
    ] {
        assert!(
            rows.iter()
                .any(|row| row["attrs"]["visibility_rule_id"] == rule),
            "missing {rule}"
        );
    }
    let tree = actual
        .visibility()
        .root()
        .as_ref()
        .ok_or_else(|| miette::miette!("missing tree"))?;
    let inline = tree
        .children()
        .iter()
        .find(|node| node.path() == "crate::inline")
        .ok_or_else(|| miette::miette!("missing inline module"))?;
    assert_eq!(
        inline.leaf_pub(),
        11,
        "use names, aliases, named items; no globs, local modules or macro bodies"
    );
    assert_eq!(inline.leaf_crate(), 12);
    assert!(!tree.children().iter().any(|node| {
        ["crate::tests", "crate::hidden", "crate::inner_hidden"].contains(&node.path().as_str())
    }));
    assert!(
        tree.children()
            .iter()
            .any(|node| node.path() == "crate::inactive")
    );
    assert_eq!(actual.visibility().snapshots().len(), 4);
    let tree_only = database.project(&target, &[]).into_diagnostic()?;
    let tree_only = tree_only
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing module-only inventory"))?;
    assert_eq!(tree_only.crate_root(), root.path());
    assert_eq!(tree_only.visibility(), actual.visibility());
    let config = root.path().join("cordial.toml");
    std::fs::write(
        &config,
        "[visibility]\nmin_module_names = 1\nmax_crate_names_for_flat = 1\nprefer_root = false\n",
    )
    .into_diagnostic()?;
    let changed = materialize(&native, root.path())?;
    assert_eq!(materialize(&legacy, root.path())?, changed);
    assert_ne!(changed, rows, "thresholds reuse the same evidence");
    std::fs::remove_file(&config).into_diagnostic()?;
    assert!(materialize(&CrateIr::new("visibility_parity"), root.path()).is_err());
    let mut corrupt = native.clone();
    corrupt
        .set_attr(
            corrupt.root(),
            cordial::ATTR_SOURCE_POLICY_INVENTORY,
            serde_json::json!({"snapshots":[]}),
        )
        .into_diagnostic()?;
    assert!(materialize(&corrupt, root.path()).is_err());
    let mut corrupt = native.clone();
    let mut payload = serde_json::to_value(&actual).into_diagnostic()?;
    payload
        .as_object_mut()
        .ok_or_else(|| miette::miette!("inventory object"))?
        .remove("visibility");
    corrupt
        .set_attr(
            corrupt.root(),
            cordial::ATTR_SOURCE_POLICY_INVENTORY,
            payload,
        )
        .into_diagnostic()?;
    assert!(
        materialize(&corrupt, root.path()).is_err(),
        "missing visibility never establishes compliance"
    );
    database
        .change_file(
            &external,
            "pub struct Leaf;\npub fn added() {}\npub mod child;\n".into(),
        )
        .into_diagnostic()?;
    let revised = database
        .project(&target, &[path.clone()])
        .into_diagnostic()?;
    assert_ne!(
        native
            .source_policy_inventory()
            .into_diagnostic()?
            .map(|i| i.visibility().clone()),
        revised
            .source_policy_inventory()
            .into_diagnostic()?
            .map(|i| i.visibility().clone())
    );
    assert_ne!(materialize(&revised, root.path())?, rows);
    database
        .change_file(&external, "pub struct Leaf;\npub mod child;\n".into())
        .into_diagnostic()?;
    let restored = database
        .project(&target, &[path.clone()])
        .into_diagnostic()?;
    assert_eq!(
        native
            .source_policy_inventory()
            .into_diagnostic()?
            .map(|i| i.visibility().clone()),
        restored
            .source_policy_inventory()
            .into_diagnostic()?
            .map(|i| i.visibility().clone())
    );
    assert_eq!(materialize(&restored, root.path())?, rows);
    database
        .change_file(&external, "pub mod broken {".into())
        .into_diagnostic()?;
    assert!(database.project(&target, &[path.clone()]).is_err());
    database
        .change_file(&external, "pub struct Leaf;\npub mod child;\n".into())
        .into_diagnostic()?;
    #[cfg(feature = "verus_ir")]
    {
        database
            .change_file(
                &path,
                "verus! { pub open spec fn proof() -> bool { true } }".into(),
            )
            .into_diagnostic()?;
        assert!(
            database.project(&target, &[path.clone()]).is_err(),
            "unsupported dialect must not undercount silently"
        );
    }
    database
        .change_file(&path, SOURCE.into())
        .into_diagnostic()?;
    assert_eq!(
        materialize(
            &database
                .project(&target, &[path.clone()])
                .into_diagnostic()?,
            root.path()
        )?,
        rows
    );
    assert_eq!(std::fs::read_to_string(&path).into_diagnostic()?, SOURCE);
    Ok(())
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn cyclic_module_paths_fail_instead_of_recursing_without_bound() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir(root.path().join("src")).into_diagnostic()?;
    std::fs::write(
        root.path().join("src/lib.rs"),
        "#[path = \"lib.rs\"] pub mod again;\n",
    )
    .into_diagnostic()?;
    assert!(
        cordial::scan_crate_visibility(root.path(), cordial::VisibilityThresholds::default())
            .is_err()
    );
    Ok(())
}
