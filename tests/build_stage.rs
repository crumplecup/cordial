use miette::{IntoDiagnostic, WrapErr};
use std::path::PathBuf;

use cordial::{
    BuildKind, StoreLayout, build_shadow_dep_rustdoc, build_workspace_members, nightly_available,
};

#[test]
fn build_caches_rustdoc_json_for_workspace_member() -> miette::Result<()> {
    cordial::init_tracing();
    if !nightly_available() {
        tracing::warn!("skipping build test: nightly toolchain required for rustdoc JSON");
        return Ok(());
    }

    let fixture = PathBuf::from("tests/fixtures/build_demo");
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let store_layout = StoreLayout::from_root(store.path(), "build_demo".to_string());

    let artifacts = build_workspace_members(&fixture, &store_layout, None, true)
        .into_diagnostic()
        .wrap_err("build workspace member")?;

    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].crate_name(), "build_demo");
    assert!(store_layout.rustdoc_cache_path("build_demo").is_file());
    assert!(store_layout.build_artifact_path("build_demo").is_file());
    assert!(fixture.join("doc/build_demo.json").is_file());
    Ok(())
}

#[test]
fn build_shadow_dep_caches_upstream_rustdoc_for_tracked_pair() -> miette::Result<()> {
    cordial::init_tracing();
    if !nightly_available() {
        tracing::warn!(
            "skipping shadow-dep build test: nightly toolchain required for rustdoc JSON"
        );
        return Ok(());
    }

    let fixture = PathBuf::from("tests/parity/workspaces/minimal-workspace");
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let store_layout = StoreLayout::from_root(store.path(), "minimal-workspace");

    let artifact = build_shadow_dep_rustdoc(&fixture, &store_layout, "elicit_url", "url", true)
        .into_diagnostic()
        .wrap_err("build shadow-dep rustdoc")?;

    assert_eq!(artifact.build_kind(), BuildKind::MemberDependency);
    assert_eq!(artifact.reference_member().as_deref(), Some("elicit_url"));
    assert!(artifact.features().contains(&"serde".to_string()));
    assert!(
        store_layout
            .shadow_dep_rustdoc_cache_path("elicit_url", "url")
            .is_file()
    );
    assert!(
        store_layout
            .shadow_dep_build_artifact_path("elicit_url", "url")
            .is_file()
    );
    Ok(())
}

#[test]
fn collect_trait_prereqs_reads_supertrait_impls() -> miette::Result<()> {
    cordial::init_tracing();
    use cordial::rustdoc::{
        collect_trait_prereqs_for_inventory, demo_impl_coverage_crate, write_rustdoc_crate_json,
    };
    use cordial::testing::parse_rustdoc_json;

    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let json_path = temp.path().join("demo.json");
    write_rustdoc_crate_json(&json_path, &demo_impl_coverage_crate())
        .into_diagnostic()
        .wrap_err("write json")?;
    let inventory = parse_rustdoc_json(&json_path, "demo")
        .into_diagnostic()
        .wrap_err("parse")?;
    let prereqs = collect_trait_prereqs_for_inventory(&inventory);
    let widget = prereqs
        .get("demo::Widget")
        .ok_or_else(|| miette::miette!("widget prereqs"))?;
    assert!(widget.serialize);
    assert!(!widget.deserialize);
    assert!(!widget.elicit_complete);
    Ok(())
}
