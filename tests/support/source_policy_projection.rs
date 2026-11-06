//! Complete policy parity and consumers that cannot access the source tree.
#[cfg(feature = "glob_imports")]
use cordial::GlobImportInventoryEnricher;
#[cfg(feature = "inline_tests")]
use cordial::InlineTestInventoryEnricher;
#[cfg(feature = "modularity")]
use cordial::ModularityInventoryEnricher;
#[cfg(feature = "pageantry")]
use cordial::PageantryInventoryEnricher;
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
    for dir in ["src/fixtures", "tests/fixtures"] {
        std::fs::create_dir_all(root.path().join(dir)).into_diagnostic()?;
    }
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"policy_parity\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n[lints.rust.unexpected_cfgs]\nlevel = \"warn\"\ncheck-cfg = ['cfg(from_manifest)']\n").into_diagnostic()?;
    let build = root.path().join("build.rs");
    let build_source = "fn main() { println!(\"cargo::rustc-check-cfg=cfg(from_build)\"); }\n";
    std::fs::write(&build, build_source).into_diagnostic()?;
    let path = root.path().join("src/lib.rs");
    std::fs::write(&path, SOURCE).into_diagnostic()?;
    let integration = root.path().join("tests/integration.rs");
    std::fs::write(&integration, "#[allow(unused)] fn integration() { use crate::helper::*; }\n#[test] fn integration_test() {}\n").into_diagnostic()?;
    std::fs::write(
        root.path().join("tests/fixtures/ignored.rs"),
        "#[allow(unused)] fn fixture() { use ignored::*; }\n#[test] fn ignored_test() {}\n",
    )
    .into_diagnostic()?;
    let source_fixture = root.path().join("src/fixtures/generated.rs");
    std::fs::write(
        &source_fixture,
        "struct FixtureType; fn fixture_worker() {}\n",
    )
    .into_diagnostic()?;
    let target = CrateTarget::new("policy_parity", root.path());
    let legacy = load(root.path(), &target)?;
    let mut database = SourceDatabase::load(root.path()).into_diagnostic()?;
    let paths = [path.clone(), integration, source_fixture];
    let native = database.project_target(&target).into_diagnostic()?;
    let expected = legacy
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing compatibility policy"))?;
    let actual = native
        .source_policy_inventory()
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing native policy"))?;
    assert_eq!(expected.snapshots(), actual.snapshots(), "policy snapshots");
    assert_eq!(expected.allows(), actual.allows(), "allow evidence");
    #[cfg(feature = "modularity")]
    assert_eq!(
        expected.modularity(),
        actual.modularity(),
        "modularity evidence"
    );
    #[cfg(feature = "pageantry")]
    assert_eq!(
        expected.pageantry(),
        actual.pageantry(),
        "pageantry evidence"
    );
    #[cfg(feature = "glob_imports")]
    assert_eq!(
        expected.glob_imports(),
        actual.glob_imports(),
        "glob evidence"
    );
    #[cfg(feature = "inline_tests")]
    assert_eq!(
        expected.inline_tests(),
        actual.inline_tests(),
        "test evidence"
    );
    assert_eq!(
        expected.cfg_names(),
        actual.cfg_names(),
        "cfg name evidence"
    );
    assert_eq!(
        expected.cfg_groups(),
        actual.cfg_groups(),
        "cfg group evidence"
    );
    assert_eq!(
        expected, actual,
        "complete policy evidence and declarations"
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
    #[cfg(all(feature = "modularity", feature = "pageantry"))]
    verify_policy_changes(&legacy, &native, root.path())?;
    #[cfg(feature = "glob_imports")]
    {
        let sites = actual.glob_imports();
        assert!(
            sites
                .iter()
                .any(|site| site.snippet() == "hidden::*" && site.context() == "imports::inactive")
        );
        assert!(
            sites
                .iter()
                .any(|site| site.snippet() == "external::nested::deeper::r#type::*")
        );
        assert!(sites.iter().any(|site| site.context() == "imports::tests"));
        assert!(
            sites
                .iter()
                .any(|site| site.file() == Path::new("tests/integration.rs"))
        );
        assert!(
            !sites.iter().any(|site| site.snippet().contains("prelude")
                || site.file().starts_with("tests/fixtures"))
        );
    }
    #[cfg(feature = "inline_tests")]
    {
        let sites = actual.inline_tests();
        assert!(
            sites
                .iter()
                .any(|site| site.context() == "imports::inactive::inactive_test")
        );
        assert!(
            sites
                .iter()
                .any(|site| site.context() == "imports::enclosing::parameterized")
        );
        assert!(!sites.iter().any(|site| site.file().starts_with("tests")
            || site.snippet().contains("hidden_test")
            || site.snippet().contains("suppressed_method")
            || site.snippet().contains("not_a_test")));
    }
    database
        .change_file(
            &path,
            SOURCE
                .replace("unknown", "changed")
                .replace("std::collections", "std::path")
                .replace("parameterized", "revised_test")
                .replace("mod size_cases", "mod revised_sizes")
                .replace(
                    "let shim_line = 1;",
                    "let shim_line = 1;\n    let extra_line = 2;",
                ),
        )
        .into_diagnostic()?;
    assert!(
        database
            .change_file(
                &build,
                "fn main() { println!(\"cargo::rustc-check-cfg=cfg(changed_build)\"); }\n".into(),
            )
            .is_err(),
        "build-script revisions require a Cargo context reload"
    );
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
    assert!(!revised.known_cfg_names().contains("changed_build"));
    assert!(revised.known_cfg_names().contains("from_build"));
    #[cfg(feature = "glob_imports")]
    {
        assert!(
            revised
                .glob_imports()
                .iter()
                .any(|site| site.snippet() == "std::path::*")
        );
        assert!(
            !revised
                .glob_imports()
                .iter()
                .any(|site| site.snippet() == "std::collections::*")
        );
    }
    #[cfg(feature = "inline_tests")]
    {
        assert!(
            revised
                .inline_tests()
                .iter()
                .any(|site| site.context().ends_with("::revised_test"))
        );
        assert!(
            !revised
                .inline_tests()
                .iter()
                .any(|site| site.context().ends_with("::parameterized"))
        );
    }
    #[cfg(feature = "modularity")]
    {
        assert_ne!(actual.modularity(), revised.modularity());
        assert!(
            revised
                .modularity()
                .iter()
                .any(|site| site.context() == "revised_sizes")
        );
        assert!(
            !revised
                .modularity()
                .iter()
                .any(|site| site.context().contains("closure_hidden")
                    || site.context().contains("test_hidden"))
        );
    }
    #[cfg(feature = "pageantry")]
    {
        assert_ne!(actual.pageantry(), revised.pageantry());
        assert!(revised.pageantry().iter().any(|site| {
            site.shim()
                .as_ref()
                .is_some_and(|shim| shim.body_lines() == 10)
        }));
    }
    assert_ne!(actual.snapshots(), revised.snapshots());
    database
        .change_file(&path, SOURCE.into())
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
        #[cfg(feature = "glob_imports")]
        &GlobImportInventoryEnricher,
        #[cfg(feature = "inline_tests")]
        &InlineTestInventoryEnricher,
        #[cfg(feature = "modularity")]
        &ModularityInventoryEnricher,
        #[cfg(feature = "pageantry")]
        &PageantryInventoryEnricher,
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
        #[cfg(feature = "glob_imports")]
        "glob_import_rule_id",
        #[cfg(feature = "inline_tests")]
        "inline_test_rule_id",
        #[cfg(feature = "modularity")]
        "modularity_kind",
        #[cfg(feature = "pageantry")]
        "pageantry_rule_id",
    ] {
        for node in actual.nodes_matching(&QueryBuilder::new().has_attr(key).build()) {
            let attrs: std::collections::BTreeMap<_, _> =
                node.weight().attrs().iter().cloned().collect();
            rows.push(serde_json::json!({"kind":key, "attrs":attrs, "span":node.weight().span()}));
        }
    }
    assert!(!rows.is_empty());
    rows.sort_by_key(serde_json::Value::to_string);
    Ok(rows)
}

/// Reconfigure the same evidence, with no source tree available to enrichers.
#[cfg(all(feature = "modularity", feature = "pageantry"))]
#[tracing::instrument(level = "debug", skip(legacy, native), err(level = "warn"))]
fn verify_policy_changes(legacy: &CrateIr, native: &CrateIr, root: &Path) -> Result<()> {
    let baseline = materialized(native, root)?;
    let config = root.join("cordial.toml");
    std::fs::write(&config, "[modularity]\nfile_inventory_min_lines = 0\nfunction_inventory_min_lines = 0\nfunction_hotspot_min_lines = 0\nmax_types_per_file = 0\ngenerated_files = ['src/lib.rs']\n[pageantry]\nbarrel = false\nmax_shim_lines = 0\n").into_diagnostic()?;
    let revised = materialized(native, root)?;
    assert_eq!(materialized(legacy, root)?, revised);
    assert_ne!(baseline, revised);
    assert!(revised.iter().any(
        |row| row["attrs"]["modularity_kind"] == "MODULARITY-FUNCTION"
            && row["attrs"]["context"] == "size_cases::First::method"
    ));
    assert!(revised.iter().any(|row| {
        row["attrs"]["pageantry_rule_id"] == "PAGEANTRY-BARREL-SHIM-001"
            && row["attrs"]["snippet"]
                .as_str()
                .is_some_and(|snippet| snippet.contains("shims may have 0"))
    }));
    assert!(
        !revised
            .iter()
            .any(|row| row["attrs"]["pageantry_rule_id"] == "PAGEANTRY-BARREL-001")
    );
    assert!(!revised.iter().any(|row| {
        row["attrs"]["modularity_kind"] == "MODULARITY-MODULE-SIZE"
            && row["attrs"]["file"]
                .as_str()
                .is_some_and(|file| Path::new(file) == root.join("src/lib.rs"))
    }));
    std::fs::remove_file(config).into_diagnostic()?;
    Ok(())
}
