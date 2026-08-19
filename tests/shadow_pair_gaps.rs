//! Cross-crate shadow mirror compare integration tests.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::testing::{ShadowStatus, build_shadow_report_from_inventories, parse_rustdoc_json};

mod parity_support;

use parity_support::{run_cordial_shadow_coverage, seed_shadow_dep_rustdoc, write_minimal_rustdoc};

#[test]
fn cross_crate_shadow_covers_widget_in_minimal_workspace() -> miette::Result<()> {
    let workspace =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/workspaces/minimal-workspace");
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    run_cordial_shadow_coverage(&workspace, store.path(), Some("url"))?;

    let pair_csv = fs::read_to_string(store.path().join("findings/shadow-url.csv"))
        .into_diagnostic()
        .wrap_err("pair csv")?;
    assert!(pair_csv.contains("url::Widget"));
    assert!(pair_csv.contains("Covered"));

    let gaps_csv = fs::read_to_string(store.path().join("findings/gaps-shadow.csv"))
        .into_diagnostic()
        .wrap_err("gaps csv")?;
    assert!(
        gaps_csv.contains("ShadowVerificationGap"),
        "expected verification gap for mirrored Widget, got:\n{gaps_csv}"
    );
    Ok(())
}

#[test]
fn method_checklist_artifact_emitted_when_maps_differ() -> miette::Result<()> {
    use std::collections::{BTreeSet, HashMap};

    use cordial::testing::{
        InventoryItemKind, ShadowBuildMaps, build_shadow_report_from_inventories_with_maps,
        parse_rustdoc_json, render_shadow_method_checklist,
    };

    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    write_minimal_rustdoc(workspace.path(), "url", "Widget")?;
    write_minimal_rustdoc(workspace.path(), "elicit_url", "Widget")?;

    let target = parse_rustdoc_json(&workspace.path().join("target/doc/url.json"), "url")
        .into_diagnostic()
        .wrap_err("target")?;
    let shadow = parse_rustdoc_json(
        &workspace.path().join("target/doc/elicit_url.json"),
        "elicit_url",
    )
    .into_diagnostic()
    .wrap_err("shadow")?;

    let mut target_methods = HashMap::new();
    target_methods.insert(
        "url::Widget".to_string(),
        BTreeSet::from(["draw".to_string(), "resize".to_string()]),
    );
    let mut shadow_methods = HashMap::new();
    shadow_methods.insert(
        "elicit_url::Widget".to_string(),
        BTreeSet::from(["draw".to_string()]),
    );
    let empty_traits: HashMap<String, BTreeSet<String>> = HashMap::new();
    let maps = ShadowBuildMaps {
        target_methods: &target_methods,
        shadow_methods: &shadow_methods,
        target_trait_impls: &empty_traits,
        shadow_trait_impls: &empty_traits,
    };

    let report = build_shadow_report_from_inventories_with_maps(&target, &shadow, &maps);
    let checklist = render_shadow_method_checklist(&report)
        .into_diagnostic()
        .wrap_err("render checklist")?;
    assert!(checklist.contains("Methods to add"));
    assert!(checklist.contains("resize"));
    assert!(checklist.contains("url::Widget"));
    assert_eq!(report.method_coverage[0].missing, vec!["resize"]);
    assert_eq!(target.items[0].kind, InventoryItemKind::Struct);
    Ok(())
}

#[test]
fn build_shadow_report_unit_exact_match() -> miette::Result<()> {
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    write_minimal_rustdoc(workspace.path(), "url", "Widget")?;
    write_minimal_rustdoc(workspace.path(), "elicit_url", "Widget")?;

    let target = parse_rustdoc_json(&workspace.path().join("target/doc/url.json"), "url")
        .into_diagnostic()
        .wrap_err("target inventory")?;
    let shadow = parse_rustdoc_json(
        &workspace.path().join("target/doc/elicit_url.json"),
        "elicit_url",
    )
    .into_diagnostic()
    .wrap_err("shadow inventory")?;
    let report = build_shadow_report_from_inventories(&target, &shadow);
    assert_eq!(report.covered_count, 1);
    assert_eq!(report.rows[0].status, ShadowStatus::Covered);
    Ok(())
}

#[test]
fn prefix_rename_is_missing_not_drift() -> miette::Result<()> {
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    write_minimal_rustdoc(workspace.path(), "url", "Vec2")?;
    write_minimal_rustdoc(workspace.path(), "elicit_url", "EguiVec2")?;

    let target = parse_rustdoc_json(&workspace.path().join("target/doc/url.json"), "url")
        .into_diagnostic()
        .wrap_err("target")?;
    let shadow = parse_rustdoc_json(
        &workspace.path().join("target/doc/elicit_url.json"),
        "elicit_url",
    )
    .into_diagnostic()
    .wrap_err("shadow")?;
    let report = build_shadow_report_from_inventories(&target, &shadow);
    assert_eq!(report.missing_count, 1);
    assert_eq!(report.extra_count, 1);
    assert_eq!(report.drifted_count, 0);
    Ok(())
}

#[test]
fn method_coverage_diffs_matched_types() -> miette::Result<()> {
    use std::collections::{BTreeSet, HashMap};

    use cordial::testing::{ShadowBuildMaps, build_shadow_report_from_inventories_with_maps};

    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    write_minimal_rustdoc(workspace.path(), "upstream", "Widget")?;
    write_minimal_rustdoc(workspace.path(), "elicit_upstream", "Widget")?;

    let target = parse_rustdoc_json(
        &workspace.path().join("target/doc/upstream.json"),
        "upstream",
    )
    .into_diagnostic()
    .wrap_err("target")?;
    let shadow = parse_rustdoc_json(
        &workspace.path().join("target/doc/elicit_upstream.json"),
        "elicit_upstream",
    )
    .into_diagnostic()
    .wrap_err("shadow")?;

    let mut target_methods = HashMap::new();
    target_methods.insert(
        "upstream::Widget".to_string(),
        BTreeSet::from(["draw".to_string(), "resize".to_string()]),
    );
    let mut shadow_methods = HashMap::new();
    shadow_methods.insert(
        "elicit_upstream::Widget".to_string(),
        BTreeSet::from(["draw".to_string(), "extra_fn".to_string()]),
    );
    let empty_traits: HashMap<String, BTreeSet<String>> = HashMap::new();
    let maps = ShadowBuildMaps {
        target_methods: &target_methods,
        shadow_methods: &shadow_methods,
        target_trait_impls: &empty_traits,
        shadow_trait_impls: &empty_traits,
    };

    let report = build_shadow_report_from_inventories_with_maps(&target, &shadow, &maps);
    assert_eq!(report.method_coverage.len(), 1);
    let coverage = &report.method_coverage[0];
    assert_eq!(coverage.covered, vec!["draw"]);
    assert_eq!(coverage.missing, vec!["resize"]);
    assert_eq!(coverage.extra, vec!["extra_fn"]);
    Ok(())
}

#[test]
fn upstream_inventory_prefers_shadow_dep_cache() -> miette::Result<()> {
    use cordial::SessionBuilder;
    use cordial::testing::build_shadow_pair_report;

    let workspace =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/workspaces/minimal-workspace");
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    run_cordial_shadow_coverage(&workspace, store.path(), Some("url"))?;

    seed_shadow_dep_rustdoc(store.path(), "elicit_url", "url", "AltWidget")?;

    let session = SessionBuilder::new(&workspace)
        .with_store_root(store.path())
        .build();
    let report = build_shadow_pair_report(&session, "url", "elicit_url")
        .into_diagnostic()
        .wrap_err("shadow pair report")?;

    let alt = report
        .rows
        .iter()
        .find(|row| row.item_path.ends_with("AltWidget"))
        .ok_or_else(|| miette::miette!("shadow-dep upstream type should drive the pair report"))?;
    assert_eq!(alt.status, ShadowStatus::Missing);
    Ok(())
}
