//! Panic enrichment needs persisted graph evidence, including proof exemptions.
#![cfg(feature = "panics")]

use cordial::{
    ATTR_SOURCE_PANICS, CrateIr, CrateTarget, EnrichView, Etiquette, IrView, LoadContext, Loader,
    PANICS_ETIQUETTE, QueryBuilder, SessionBuilder, SourceLoadView, SourceLoader, SourcePanics,
};
use miette::{IntoDiagnostic, Result};
use sha2::{Digest, Sha256};

#[test]
#[tracing::instrument(level = "debug", err(level = "warn"))]
fn panic_inventory_survives_source_removal_and_rejects_incomplete_evidence() -> Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    for (name, text) in [
        ("lib.rs", include_str!("fixtures/panics/graph_root.rs")),
        ("leaf.rs", include_str!("fixtures/panics/graph_leaf.rs")),
        ("middle.rs", include_str!("fixtures/panics/graph_middle.rs")),
        ("empty.rs", "// no panic sites\n"),
    ] {
        std::fs::write(root.join("src").join(name), text).into_diagnostic()?;
    }
    std::fs::create_dir_all(root.join("tests/fixtures")).into_diagnostic()?;
    std::fs::create_dir_all(root.join("tests/parity/src")).into_diagnostic()?;
    for path in ["tests/fixtures/ignored.rs", "tests/parity/src/lib.rs"] {
        std::fs::write(
            root.join(path),
            include_str!("fixtures/panics/sample_27.rs"),
        )
        .into_diagnostic()?;
    }
    let session = SessionBuilder::new(root).build();
    let target = CrateTarget::new("panic_evidence", root);
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("source view"))?;
    let mut graph = CrateIr::new("panic_evidence");
    source.populate_ir(&mut graph).into_diagnostic()?;
    let evidence = graph.source_panics().into_diagnostic()?;
    assert_eq!(
        evidence.snapshots().len(),
        4,
        "empty files are evidence too"
    );
    for snapshot in evidence.snapshots() {
        let digest: [u8; 32] =
            Sha256::digest(std::fs::read(snapshot.path()).into_diagnostic()?).into();
        assert_eq!(snapshot.digest(), digest);
    }
    let proof = evidence.proof_graph();
    assert_eq!(
        proof.roots().iter().map(String::as_str).collect::<Vec<_>>(),
        ["root", "wrapped"]
    );
    assert_eq!(
        proof.calls().get("root"),
        Some(&vec!["middle".into(), "runtime_branch".into()])
    );
    assert_eq!(proof.calls().get("middle"), Some(&vec!["leaf".into()]));
    for reachable in [
        "leaf",
        "middle",
        "root",
        "wrapped",
        "cycle_a",
        "cycle_b",
        "runtime_branch",
    ] {
        assert!(
            proof.reachable().contains(reachable),
            "missing proof edge: {reachable}"
        );
    }
    assert!(!proof.reachable().contains("manual"));
    assert_eq!(evidence.records().len(), 4, "{:#?}", evidence.records());
    assert!(
        evidence
            .records()
            .iter()
            .any(|site| site.context() == "leaf::runtime_branch")
    );
    assert!(
        evidence
            .records()
            .iter()
            .any(|site| site.context() == "fixture_host::embedded")
    );
    assert_eq!(
        evidence
            .records()
            .iter()
            .filter(|site| site.cfg_test())
            .count(),
        1
    );
    let encoded = serde_json::to_value(&evidence).into_diagnostic()?;
    assert_eq!(
        serde_json::from_value::<SourcePanics>(encoded.clone()).into_diagnostic()?,
        evidence
    );
    let mut incomplete = encoded;
    incomplete
        .as_object_mut()
        .ok_or_else(|| miette::miette!("object"))?
        .remove("proof_graph");
    assert!(serde_json::from_value::<SourcePanics>(incomplete).is_err());
    assert!(CrateIr::new("missing").source_panics().is_err());
    let mut corrupted = graph.clone();
    corrupted
        .set_attr(
            corrupted.root(),
            ATTR_SOURCE_PANICS,
            serde_json::json!({"records": []}),
        )
        .into_diagnostic()?;
    assert!(corrupted.source_panics().is_err());
    let expected = materialize(&graph)?;
    assert_eq!(expected.len(), evidence.records().len());
    drop(loaded);
    std::fs::remove_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::remove_dir_all(root.join("tests")).into_diagnostic()?;
    assert_eq!(graph.source_panics().into_diagnostic()?, evidence);
    assert_eq!(
        materialize(&graph)?,
        expected,
        "full attributes and spans survive source deletion"
    );
    Ok(())
}

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
fn materialize(graph: &CrateIr) -> Result<Vec<serde_json::Value>> {
    let empty = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(empty.path()).build();
    let target = CrateTarget::new("empty", empty.path());
    let loaded = SourceLoader
        .load(LoadContext::new(&session, &target))
        .into_diagnostic()?;
    let hook = PANICS_ETIQUETTE
        .enrichers()
        .iter()
        .find(|hook| hook.id() == "panic-inventory")
        .ok_or_else(|| miette::miette!("panic hook"))?;
    let mut graph = graph.clone();
    hook.enrich(EnrichView::new(&mut graph, loaded.as_ref(), &session))
        .into_diagnostic()?;
    let mut rows = Vec::new();
    for node in graph.nodes_matching(&QueryBuilder::new().has_attr("panic_kind").build()) {
        let attrs: std::collections::BTreeMap<_, _> =
            node.weight().attrs().iter().cloned().collect();
        rows.push(serde_json::json!({"attrs": attrs, "span": node.weight().span()}));
    }
    rows.sort_by_key(serde_json::Value::to_string);
    Ok(rows)
}
