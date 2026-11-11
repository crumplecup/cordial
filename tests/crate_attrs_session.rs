//! The crate-attribute lint in a session: rule switches, per-crate checklists, and the dogfood scan.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{
    CRATE_ATTRS_ETIQUETTE, CrateAttrsRuleId, CrateAttrsThresholds, RunAll, Session, SessionBuilder,
    scan_crate_attrs,
};

#[path = "support/crate_attrs_fixture.rs"]
mod crate_attrs_fixture;

use crate_attrs_fixture::write_package;
fn has(records: &[cordial::CrateAttrsSiteRecord], rule: CrateAttrsRuleId) -> bool {
    records.iter().any(|record| record.rule_id() == rule)
}

#[test]
fn toggling_a_rule_off_skips_it_everywhere() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_package(fixture.path(), "fixture", "pub fn ready() {}\n")?;
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        "[crate_attrs]\nforbid_unsafe = false\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    let loaded = cordial::load_cordial_config(workspace.path(), store_home.path());
    let records =
        scan_crate_attrs(fixture.path(), "fixture", loaded.crate_attrs()).into_diagnostic()?;
    assert!(!has(&records, CrateAttrsRuleId::ForbidUnsafe001));
    assert!(has(&records, CrateAttrsRuleId::MissingDocs001));
    Ok(())
}

#[test]
fn session_flags_bare_lib() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_package(fixture.path(), "bare", "pub fn ready() {}\n")?;
    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .register(&*CRATE_ATTRS_ETIQUETTE)
        .build();
    let outcome = session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let ids: Vec<_> = outcome
        .findings()
        .map(|finding| finding.rule().id())
        .collect();
    assert!(
        ids.contains(&"CRATE-FORBID-UNSAFE-001"),
        "session should flag forbid: {ids:?}"
    );
    assert!(
        ids.contains(&"CRATE-MISSING-DOCS-001"),
        "session should flag docs: {ids:?}"
    );
    Ok(())
}

fn write_member(workspace: &Path, rel: &str, name: &str, lib_body: &str) -> miette::Result<()> {
    let crate_root = workspace.join(rel);
    fs::create_dir_all(crate_root.join("src"))
        .into_diagnostic()
        .wrap_err("member src")?;
    fs::write(
        crate_root.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
    )
    .into_diagnostic()
    .wrap_err("member manifest")?;
    fs::write(crate_root.join("src/lib.rs"), lib_body)
        .into_diagnostic()
        .wrap_err("lib rs")?;
    Ok(())
}

#[test]
fn checklist_groups_by_each_crate() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/alpha\", \"crates/ffi\"]\nresolver = \"2\"\n",
    )
    .into_diagnostic()
    .wrap_err("workspace manifest")?;
    write_member(fixture.path(), "crates/alpha", "alpha", "pub fn a() {}\n")?;
    write_member(fixture.path(), "crates/ffi", "ffi", "pub fn f() {}\n")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[crate_attrs]\nallow_unsafe = [\"ffi\"]\nallow_missing_docs = [\"ffi\"]\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(fixture.path())
        .with_store_root(store.path())
        .with_store_home(store.path())
        .register(&*CRATE_ATTRS_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;

    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("crate-attrs.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(
        checklist.contains("## `alpha`"),
        "alpha should be listed: {checklist}"
    );
    assert!(
        !checklist.contains("## `ffi`"),
        "ffi is fully excepted: {checklist}"
    );
    assert!(checklist.contains("CRATE-FORBID-UNSAFE-001"));
    assert!(checklist.contains("CRATE-MISSING-DOCS-001"));
    Ok(())
}

#[test]
fn crate_attrs_default_thresholds() {
    cordial::init_tracing();
    let thresholds = CrateAttrsThresholds::default();
    assert!(thresholds.forbid_unsafe());
    assert!(thresholds.missing_docs());
    assert!(thresholds.allow_unsafe().is_empty());
    assert!(thresholds.allow_missing_docs().is_empty());
}

// Slow (over 60s): compiled only without the default `quick` feature; run with `just test-slow`.
#[cfg(not(feature = "quick"))]
#[test]
fn dogfood_cordial_library_root() -> miette::Result<()> {
    cordial::init_tracing();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let records = scan_crate_attrs(root, "cordial", &CrateAttrsThresholds::default())
        .into_diagnostic()
        .wrap_err("scan cordial")?;
    assert!(
        records.is_empty(),
        "cordial src/lib.rs should declare both crate-root attrs: {records:?}"
    );

    let store = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("store tempdir")?;
    let session = SessionBuilder::new(root)
        .with_store_root(store.path())
        .register(&*CRATE_ATTRS_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("session run")?;
    let checklist = fs::read_to_string(
        store
            .path()
            .join("findings")
            .join("crate-attrs.checklist.md"),
    )
    .into_diagnostic()
    .wrap_err("checklist")?;
    assert!(
        checklist.contains("**Open items:** 0"),
        "cordial should have no crate-attr findings: {checklist}"
    );
    assert!(
        checklist.contains("_No crate-attribute findings._"),
        "empty checklist should say so: {checklist}"
    );

    let before = fs::read_to_string(root.join("src/lib.rs")).into_diagnostic()?;
    let summary = run_crate_attrs_apply(root, store.path(), None, true).into_diagnostic()?;
    let after = fs::read_to_string(root.join("src/lib.rs")).into_diagnostic()?;
    assert_eq!(before, after, "dry-run must not rewrite cordial src/lib.rs");
    assert_eq!(summary.inserted_attrs(), 0);
    assert_eq!(summary.changed_files(), 0);
    Ok(())
}
