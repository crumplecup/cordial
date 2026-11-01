//! Complete policy parity and consumers that cannot access the source tree.
use cordial::{
    AllowInventoryEnricher, CfgHygieneInventoryEnricher, CfgScatterInventoryEnricher, CrateIr,
    CrateTarget, IrView, LoadContext, Loader, QueryBuilder, SessionBuilder, SourceDatabase,
    SourceEnrichView, SourceEnricher, SourceLoadView, SourceLoader,
};
use miette::{IntoDiagnostic, Result};
use std::path::Path;

const SOURCE: &str = include_str!("../fixtures/semantic_backend/policy.rs.txt");

#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn verify() -> Result<()> {
    let root = tempfile::tempdir().into_diagnostic()?;
    for dir in ["src", "tests/fixtures"] {
        std::fs::create_dir_all(root.path().join(dir)).into_diagnostic()?;
    }
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"policy_parity\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n[lints.rust.unexpected_cfgs]\nlevel = \"warn\"\ncheck-cfg = ['cfg(from_manifest)']\n").into_diagnostic()?;
    let build = root.path().join("build.rs");
    let build_source = "fn main() { println!(\"cargo::rustc-check-cfg=cfg(from_build)\"); }\n";
    std::fs::write(&build, build_source).into_diagnostic()?;
    let path = root.path().join("src/lib.rs");
    std::fs::write(&path, SOURCE).into_diagnostic()?;
    let integration = root.path().join("tests/integration.rs");
    std::fs::write(&integration, "#[allow(unused)] fn integration() {}\n").into_diagnostic()?;
    std::fs::write(
        root.path().join("tests/fixtures/ignored.rs"),
        "#[allow(unused)] fn fixture() {}\n",
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("policy_parity", root.path());
    let legacy = load(root.path(), &target)?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let paths = [path.clone(), integration];
    let native = database.project(&target, &paths).into_diagnostic()?;
    let expected = legacy
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing compatibility policy"))?;
    let actual = native
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing native policy"))?;
    assert_eq!(
        expected, actual,
        "complete policy evidence, snapshots and declarations"
    );
    assert!(actual.known_cfg_names().contains("from_build"));
    assert!(actual.known_cfg_names().contains("from_manifest"));
    assert!(
        !actual
            .cfg_names()
            .iter()
            .any(|site| site.name() == "macro_payload")
    );
    assert!(
        !actual
            .allows()
            .iter()
            .any(|site| site.context().contains("ignored_allow"))
    );
    assert!(
        actual
            .allows()
            .iter()
            .any(|site| site.file() == Path::new("tests/integration.rs"))
    );
    assert!(
        !actual
            .allows()
            .iter()
            .any(|site| site.file().starts_with("tests/fixtures"))
    );
    assert_eq!(
        materialized(&legacy, root.path())?,
        materialized(&native, root.path())?
    );
    database
        .change_file(&path, SOURCE.replace("unknown", "changed"))
        .into_diagnostic()?;
    database
        .change_file(
            &build,
            "fn main() { println!(\"cargo::rustc-check-cfg=cfg(changed_build)\"); }\n".into(),
        )
        .into_diagnostic()?;
    let revised = database.project(&target, &paths).into_diagnostic()?;
    let revised = revised
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing revised policy"))?;
    assert!(
        revised
            .cfg_names()
            .iter()
            .any(|site| site.name() == "changed")
    );
    assert!(
        !revised
            .cfg_names()
            .iter()
            .any(|site| site.name() == "unknown")
    );
    assert!(revised.known_cfg_names().contains("changed_build"));
    assert!(!revised.known_cfg_names().contains("from_build"));
    assert_ne!(actual.snapshots(), revised.snapshots());
    database
        .change_file(&path, SOURCE.into())
        .into_diagnostic()?;
    database
        .change_file(&build, build_source.into())
        .into_diagnostic()?;
    let restored = database.project(&target, &paths).into_diagnostic()?;
    assert_eq!(
        native.source_policy_inventory().into_diagnostic()?,
        restored.source_policy_inventory().into_diagnostic()?
    );
    assert_eq!(std::fs::read_to_string(&path).into_diagnostic()?, SOURCE);
    Ok(())
}

#[tracing::instrument(level = "debug", skip(target), err(level = "warn"))]
fn load(root: &Path, target: &CrateTarget) -> Result<CrateIr> {
    let session = SessionBuilder::new(root).build();
    let loaded = SourceLoader
        .load(LoadContext::new(&session, target))
        .into_diagnostic()?;
    let source = loaded
        .as_any()
        .downcast_ref::<SourceLoadView>()
        .ok_or_else(|| miette::miette!("unexpected loader"))?;
    let mut graph = CrateIr::new("policy_parity");
    source.populate_ir(&mut graph).into_diagnostic()?;
    Ok(graph)
}

#[tracing::instrument(level = "debug", skip(graph), err(level = "warn"))]
fn materialized(graph: &CrateIr, root: &Path) -> Result<Vec<serde_json::Value>> {
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
    let mut actual = graph.clone();
    for hook in [
        &AllowInventoryEnricher as &dyn SourceEnricher,
        &CfgHygieneInventoryEnricher,
        &CfgScatterInventoryEnricher,
    ] {
        // Missing and corrupt evidence must fail before writing any policy node.
        let mut missing = CrateIr::new("missing");
        assert!(
            hook.enrich_source(SourceEnrichView::new(&mut missing, source, &session))
                .is_err()
        );
        let mut corrupt = graph.clone();
        corrupt
            .set_attr(
                corrupt.root(),
                cordial::ATTR_SOURCE_POLICY_INVENTORY,
                serde_json::json!({"snapshots": []}),
            )
            .into_diagnostic()?;
        assert!(
            hook.enrich_source(SourceEnrichView::new(&mut corrupt, source, &session))
                .is_err()
        );
        hook.enrich_source(SourceEnrichView::new(&mut actual, source, &session))
            .into_diagnostic()?;
    }
    let mut rows = Vec::new();
    for key in [
        "allow_rule_id",
        "cfg_hygiene_rule_id",
        "cfg_scatter_predicate",
    ] {
        for node in actual.nodes_matching(&QueryBuilder::new().has_attr(key).build()) {
            rows.push(serde_json::json!({"kind":key, "attrs":node.weight().attrs(), "span":node.weight().span()}));
        }
    }
    assert!(!rows.is_empty());
    rows.sort_by_key(serde_json::Value::to_string);
    Ok(rows)
}
