use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    RunAll, Session, SessionBuilder, VISIBILITY_ETIQUETTE, VisibilityRecord, VisibilityRuleId,
    VisibilityThresholds, load_visibility_thresholds, scan_crate_visibility,
    scan_crate_visibility_with_cache,
};

fn write_crate(root: &std::path::Path, lib: &str, extra: &[(&str, &str)]) -> miette::Result<()> {
    fs::create_dir_all(root.join("src"))
        .into_diagnostic()
        .wrap_err("src")?;
    fs::write(root.join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("lib.rs")?;
    for (rel, body) in extra {
        let path = root.join("src").join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .into_diagnostic()
                .wrap_err("parent")?;
        }
        fs::write(path, body)
            .into_diagnostic()
            .wrap_err("module file")?;
    }
    Ok(())
}

#[test]
fn small_crate_with_pub_mod_flags_flat_and_thin_when_thresholds_say_so() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_crate(
        fixture.path(),
        "pub mod widgets;\npub fn root_item() {}\n",
        &[("widgets.rs", "pub fn widget() {}\n")],
    )?;

    let tight = VisibilityThresholds::default();
    let records = scan_crate_visibility(fixture.path(), tight)
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        records
            .iter()
            .any(|r| r.rule_id == VisibilityRuleId::CrateFlat001
                && r.module_path == "crate::widgets"),
        "small crate must not grow a pub mod: {records:?}"
    );
    assert!(
        records
            .iter()
            .any(|r| r.rule_id == VisibilityRuleId::ModThin001
                && r.module_path == "crate::widgets"),
        "widgets has one name, below floor 10: {records:?}"
    );
    Ok(())
}

#[test]
fn call_site_thresholds_can_silence_the_same_tree() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_crate(
        fixture.path(),
        "pub mod widgets;\npub fn root_item() {}\n",
        &[("widgets.rs", "pub fn widget() {}\n")],
    )?;

    let loose = VisibilityThresholds::new(1, 1);
    let records = scan_crate_visibility(fixture.path(), loose)
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        records.is_empty(),
        "caller-set floor 1 / crate-flat 1 should accept this tree: {records:?}"
    );
    Ok(())
}

#[test]
fn pub_mod_under_private_mod_is_mismatch() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_crate(
        fixture.path(),
        "mod inner;\npub use inner::Visible;\n",
        &[(
            "inner.rs",
            "pub fn Visible() {}\npub mod hole {\n    pub fn x() {}\n}\n",
        )],
    )?;

    let records = scan_crate_visibility(fixture.path(), VisibilityThresholds::default())
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        records.iter().any(
            |r| r.rule_id == VisibilityRuleId::ModMismatch001 && r.module_path.contains("hole")
        ),
        "pub mod under private inner must flag mismatch: {records:?}"
    );
    Ok(())
}

#[test]
fn private_mod_with_reexport_is_not_a_path() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_crate(
        fixture.path(),
        "mod inner;\npub use inner::Visible;\n",
        &[("inner.rs", "pub fn Visible() {}\n")],
    )?;

    let records = scan_crate_visibility(fixture.path(), VisibilityThresholds::default())
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        records.is_empty(),
        "private child + root pub use is the recommended shape: {records:?}"
    );
    Ok(())
}

#[test]
fn project_config_overrides_default_thresholds() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_crate(
        fixture.path(),
        "pub mod widgets;\npub fn root_item() {}\n",
        &[("widgets.rs", "pub fn widget() {}\n")],
    )?;
    fs::write(
        fixture.path().join("cordial.toml"),
        r#"
[visibility]
max_crate_names_for_flat = 0
min_module_names = 100
"#,
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let loaded = load_visibility_thresholds(fixture.path(), fixture.path());
    assert_eq!(loaded.max_crate_names_for_flat, 0);
    assert_eq!(loaded.min_module_names, 100);
    assert!(loaded.prefer_root);

    let records = scan_crate_visibility(fixture.path(), loaded)
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        records
            .iter()
            .any(|r| r.rule_id == VisibilityRuleId::ModThin001),
        "config min_module_names=100 must flag the one-name pub mod: {records:?}"
    );
    assert!(
        records
            .iter()
            .all(|r| r.rule_id != VisibilityRuleId::CrateFlat001),
        "max_crate_names_for_flat=0 means never require a flat crate: {records:?}"
    );
    Ok(())
}

#[test]
fn visibility_etiquette_session_reads_project_config() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_crate(
        fixture.path(),
        "mod inner;\npub use inner::Visible;\npub mod hole;\n",
        &[
            ("inner.rs", "pub fn Visible() {}\n"),
            ("hole.rs", "pub fn x() {}\n"),
        ],
    )?;
    fs::write(
        fixture.path().join("cordial.toml"),
        r#"
[visibility]
max_crate_names_for_flat = 50
min_module_names = 10
"#,
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&VISIBILITY_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic().wrap_err("run")?;
    let findings: Vec<_> = outcome.findings().collect();
    assert!(
        findings
            .iter()
            .any(|f| f.rule().id() == "VIS-CRATE-FLAT-001" || f.rule().id() == "VIS-MOD-THIN-001"),
        "session should surface visibility findings from project config"
    );
    Ok(())
}

fn pub_fns(prefix: &str, n: usize) -> String {
    (0..n)
        .map(|i| format!("pub fn {prefix}_{i}() {{}}\n"))
        .collect()
}

fn write_thin_overflow_crate(root: &std::path::Path, root_fns: usize) -> miette::Result<()> {
    let lib = format!(
        "pub mod a;\npub mod b;\npub mod c;\npub mod d;\n{}",
        pub_fns("root", root_fns)
    );
    let a = pub_fns("a", 9);
    let b = pub_fns("b", 7);
    let c = pub_fns("c", 7);
    let d = pub_fns("d", 6);
    write_crate(
        root,
        &lib,
        &[("a.rs", &a), ("b.rs", &b), ("c.rs", &c), ("d.rs", &d)],
    )
}

fn thin_paths(records: &[VisibilityRecord]) -> Vec<&str> {
    records
        .iter()
        .filter(|r| r.rule_id == VisibilityRuleId::ModThin001)
        .map(|r| r.module_path.as_str())
        .collect()
}

#[test]
fn prefer_root_accepts_fat_root_and_still_flags_thin_modules() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_thin_overflow_crate(fixture.path(), 40)?;
    let thresholds = VisibilityThresholds::default();
    assert!(thresholds.prefer_root);
    let records = scan_crate_visibility(fixture.path(), thresholds)
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        records
            .iter()
            .all(|r| r.rule_id != VisibilityRuleId::CrateFlat001),
        "68-name crate is above max; fat root is the preferred resolution: {records:?}"
    );
    let thin = thin_paths(&records);
    assert_eq!(thin, ["crate::a", "crate::b", "crate::c", "crate::d"]);
    Ok(())
}

#[test]
fn prefer_root_false_peels_biggest_mods_and_lowers_thin_floor() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_thin_overflow_crate(fixture.path(), 40)?;
    let thresholds = VisibilityThresholds::default().with_prefer_root(false);
    let records = scan_crate_visibility(fixture.path(), thresholds)
        .into_diagnostic()
        .wrap_err("scan")?;
    let thin = thin_paths(&records);
    assert_eq!(
        thin,
        ["crate::d"],
        "peel 9 then 7 then 7 (remaining 46 < 50); floor 7 still flags the 6: {records:?}"
    );
    Ok(())
}

#[test]
fn prefer_root_false_peels_until_root_is_under_max() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_thin_overflow_crate(fixture.path(), 44)?;
    let thresholds = VisibilityThresholds::default().with_prefer_root(false);
    let records = scan_crate_visibility(fixture.path(), thresholds)
        .into_diagnostic()
        .wrap_err("scan")?;
    assert!(
        thin_paths(&records).is_empty(),
        "peel 9, 7, 7, 6 (remaining 50 forces the last peel); floor 6 silences all four: {records:?}"
    );
    Ok(())
}

#[test]
fn branching_cache_reuses_floor_until_sources_change() -> miette::Result<()> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_thin_overflow_crate(fixture.path(), 40)?;
    let thresholds = VisibilityThresholds::default().with_prefer_root(false);
    let (first, cache) = scan_crate_visibility_with_cache(fixture.path(), thresholds, None)
        .into_diagnostic()
        .wrap_err("scan")?;
    let cache = cache.ok_or_else(|| miette::miette!("branching writes a cache"))?;
    assert_eq!(cache.floor, 7);
    let (second, reused) =
        scan_crate_visibility_with_cache(fixture.path(), thresholds, Some(cache.clone()))
            .into_diagnostic()
            .wrap_err("cached scan")?;
    let reused = reused.ok_or_else(|| miette::miette!("cache hit"))?;
    assert_eq!(reused.digest, cache.digest);
    assert_eq!(thin_paths(&second), thin_paths(&first));

    std::fs::write(fixture.path().join("src/d.rs"), pub_fns("d", 8))
        .into_diagnostic()
        .wrap_err("edit")?;
    let (third, recomputed) =
        scan_crate_visibility_with_cache(fixture.path(), thresholds, Some(cache))
            .into_diagnostic()
            .wrap_err("re-peel")?;
    let recomputed = recomputed.ok_or_else(|| miette::miette!("digest mismatch recomputes"))?;
    assert_ne!(recomputed.digest, reused.digest);
    assert!(
        thin_paths(&third).is_empty(),
        "after edit, 8 >= floor 7 so d is no longer thin: {third:?}"
    );
    Ok(())
}
