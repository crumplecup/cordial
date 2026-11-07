//! Complete derive evidence, graph-only consumers and revision recovery.
#![cfg(feature = "derives")]
// Slow (over 60s): compiled only without the default `quick` feature; run with `just test-slow`.
#![cfg(not(feature = "quick"))]
use cordial::{
    CrateIr, CrateTarget, CrateViewMut, DeriveInventoryEnricher, IrView, LoadContext, Loader,
    PathInclusionFacts, QueryBuilder, SessionBuilder, SourceDatabase, SourceEnrichView,
    SourceEnricher, SourceLoadView, SourceLoader, WorkspaceIr,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const SOURCE: &str = include_str!("fixtures/semantic_backend/derives.rs.txt");

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
    let mut graph = CrateIr::new("derive_parity");
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
    workspace.set_path_inclusions(PathInclusionFacts::default());
    let mut view = CrateViewMut::new(&mut workspace, graph.crate_name().into());
    DeriveInventoryEnricher
        .enrich_source(SourceEnrichView::new(&mut view, source, &session))
        .into_diagnostic()?;
    let mut rows = Vec::new();
    for node in view.nodes_matching(&QueryBuilder::new().has_attr("derive_rule_id").build()) {
        let attrs: std::collections::BTreeMap<_, _> =
            node.weight().attrs().iter().cloned().collect();
        rows.push(serde_json::json!({"attrs":attrs, "span":node.weight().span()}));
    }
    rows.sort_by_key(serde_json::Value::to_string);
    Ok(rows)
}

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn derive_facts_and_policy_survive_backend_changes_and_virtual_edits() -> Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(root.path().join("src/fixtures")).into_diagnostic()?;
    std::fs::create_dir(root.path().join("tests")).into_diagnostic()?;
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"derive_parity\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n").into_diagnostic()?;
    let path = root.path().join("src/lib.rs");
    let fixture = root.path().join("src/fixtures/generated.rs");
    let integration = root.path().join("tests/integration.rs");
    std::fs::write(&path, SOURCE).into_diagnostic()?;
    std::fs::write(&fixture, "struct Fixture { pub value: u8 }\n").into_diagnostic()?;
    std::fs::write(&integration, "struct Integration { pub ignored: u8 }\n").into_diagnostic()?;
    let target = CrateTarget::new("derive_parity", root.path());
    let legacy = baseline(root.path(), &target)?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let paths = [path.clone(), fixture, integration];
    let native = database.project(&target, &paths).into_diagnostic()?;
    let expected = legacy
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing compatibility facts"))?;
    let actual = native
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing native facts"))?;
    assert_payload(
        &serde_json::to_value(expected.derives()).into_diagnostic()?,
        &serde_json::to_value(actual.derives()).into_diagnostic()?,
        "derives",
    );
    assert_eq!(
        actual.derives().len(),
        2,
        "source fixtures included; integration tests excluded"
    );
    let rows = materialize(&native, root.path())?;
    assert_eq!(
        materialize(&legacy, root.path())?,
        rows,
        "all finding attributes and spans"
    );
    for rule in [
        "DERIVE-GETTER-001",
        "DERIVE-SETTER-001",
        "DERIVE-NEW-001",
        "DERIVE-USE-BUILDER-001",
        "DERIVE-BUILDER-001",
        "DERIVE-ASSTR-001",
        "DERIVE-ASREF-001",
        "DERIVE-PUB-FIELD-001",
    ] {
        assert!(
            rows.iter()
                .any(|row| row["attrs"]["derive_rule_id"] == rule),
            "missing {rule}"
        );
    }
    assert!(!rows.iter().any(
        |row| row["attrs"]["qualified_name"].as_str().is_some_and(|name| {
            [
                "CustomBuilder",
                "GeneratedBuilder",
                "Already",
                "Hidden",
                "Transparent",
                "Const",
                "Caller::new",
                "Error::new",
                "Proof",
                "NotCollected",
            ]
            .iter()
            .any(|excluded| {
                name == *excluded
                    || name.starts_with(&format!("{excluded}::"))
                    || name.split("::").any(|part| part == *excluded)
            })
        })
    ));
    assert!(
        rows.iter()
            .any(|row| row["attrs"]["qualified_name"] == "inactive::Record::value")
    );
    let config = root.path().join("cordial.toml");
    std::fs::write(
        &config,
        "[derives]\nmax_constructor_args = 0\nmin_fluent_setters = 99\n",
    )
    .into_diagnostic()?;
    let changed = materialize(&native, root.path())?;
    assert_eq!(materialize(&legacy, root.path())?, changed);
    assert!(
        !changed
            .iter()
            .any(|row| row["attrs"]["derive_rule_id"] == "DERIVE-NEW-001")
    );
    assert!(changed.iter().any(
        |row| row["attrs"]["derive_rule_id"] == "DERIVE-USE-BUILDER-001"
            && row["attrs"]["qualified_name"] == "Constructed::new"
    ));
    assert!(
        !changed
            .iter()
            .any(|row| row["attrs"]["derive_rule_id"] == "DERIVE-BUILDER-001"
                && row["attrs"]["qualified_name"] == "Widget")
    );
    std::fs::remove_file(&config).into_diagnostic()?;
    assert!(materialize(&CrateIr::new("derive_parity"), root.path()).is_err());
    let mut corrupt = native.clone();
    corrupt
        .set_attr(
            corrupt.root(),
            cordial::ATTR_SOURCE_POLICY_INVENTORY,
            serde_json::json!({"snapshots":[]}),
        )
        .into_diagnostic()?;
    assert!(materialize(&corrupt, root.path()).is_err());
    database
        .change_file(&path, SOURCE.replace("&self.name", "self.name.as_ref()"))
        .into_diagnostic()?;
    let revised = database.project(&target, &paths).into_diagnostic()?;
    assert_ne!(
        native.source_policy_inventory().into_diagnostic()?,
        revised.source_policy_inventory().into_diagnostic()?
    );
    let changed = materialize(&revised, root.path())?;
    assert!(
        !changed
            .iter()
            .any(|row| row["attrs"]["qualified_name"] == "Widget::name"
                && row["attrs"]["derive_rule_id"] == "DERIVE-GETTER-001")
    );
    assert!(
        changed
            .iter()
            .any(|row| row["attrs"]["qualified_name"] == "Widget::name"
                && row["attrs"]["derive_rule_id"] == "DERIVE-ASREF-001")
    );
    database
        .change_file(&path, SOURCE.into())
        .into_diagnostic()?;
    let restored = database.project(&target, &paths).into_diagnostic()?;
    assert_eq!(
        native.source_policy_inventory().into_diagnostic()?,
        restored.source_policy_inventory().into_diagnostic()?
    );
    assert_eq!(materialize(&restored, root.path())?, rows);
    assert_eq!(std::fs::read_to_string(path).into_diagnostic()?, SOURCE);
    Ok(())
}

/// Compare every field, reporting the first mismatching JSON path.
#[tracing::instrument(level = "trace", skip(expected, actual))]
fn assert_payload(expected: &serde_json::Value, actual: &serde_json::Value, path: &str) {
    match (expected, actual) {
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            assert_eq!(left.len(), right.len(), "{path}: array length");
            for (index, (left, right)) in left.iter().zip(right).enumerate() {
                assert_payload(left, right, &format!("{path}[{index}]"));
            }
        }
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
            assert_eq!(
                left.keys().collect::<Vec<_>>(),
                right.keys().collect::<Vec<_>>(),
                "{path}: object keys"
            );
            for (key, left) in left {
                assert_payload(left, &right[key], &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(expected, actual, "{path}"),
    }
}
