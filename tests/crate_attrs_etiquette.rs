use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{
    CRATE_ATTRS_ETIQUETTE, CrateAttrsRuleId, CrateAttrsThresholds, RunAll, Session, SessionBuilder,
    library_root_rs, run_crate_attrs_apply, scan_crate_attrs,
};

const BOTH: &str = r#"
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub fn ready() {}
"#;

fn write_lib(root: &Path, body: &str) -> miette::Result<()> {
    fs::create_dir_all(root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(root.join("src/lib.rs"), body)
        .into_diagnostic()
        .wrap_err("write lib")?;
    Ok(())
}

fn write_package(root: &Path, name: &str, lib_body: &str) -> miette::Result<()> {
    fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
    )
    .into_diagnostic()
    .wrap_err("manifest")?;
    write_lib(root, lib_body)
}

fn has(records: &[cordial::CrateAttrsSiteRecord], rule: CrateAttrsRuleId) -> bool {
    records.iter().any(|record| record.rule_id == rule)
}

#[test]
fn scan_flags_both_when_library_root_is_bare() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_lib(fixture.path(), "pub fn ready() {}\n")?;
    let records = scan_crate_attrs(fixture.path(), "fixture", &CrateAttrsThresholds::default())
        .into_diagnostic()?;
    assert!(
        has(&records, CrateAttrsRuleId::ForbidUnsafe001),
        "bare lib missing forbid: {records:?}"
    );
    assert!(
        has(&records, CrateAttrsRuleId::MissingDocs001),
        "bare lib missing docs: {records:?}"
    );
    Ok(())
}

#[test]
fn scan_accepts_warn_docs_and_forbid_unsafe() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_lib(fixture.path(), BOTH)?;
    let records = scan_crate_attrs(fixture.path(), "fixture", &CrateAttrsThresholds::default())
        .into_diagnostic()?;
    assert!(records.is_empty(), "compliant lib: {records:?}");
    Ok(())
}

#[test]
fn deny_unsafe_code_is_not_forbid() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_lib(
        fixture.path(),
        "#![deny(unsafe_code)]\n#![warn(missing_docs)]\n",
    )?;
    let records = scan_crate_attrs(fixture.path(), "fixture", &CrateAttrsThresholds::default())
        .into_diagnostic()?;
    assert!(
        has(&records, CrateAttrsRuleId::ForbidUnsafe001),
        "deny is weaker than forbid: {records:?}"
    );
    assert!(
        !has(&records, CrateAttrsRuleId::MissingDocs001),
        "docs warn is present: {records:?}"
    );
    Ok(())
}

#[test]
fn deny_missing_docs_counts() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_lib(
        fixture.path(),
        "#![forbid(unsafe_code)]\n#![deny(missing_docs)]\n",
    )?;
    let records = scan_crate_attrs(fixture.path(), "fixture", &CrateAttrsThresholds::default())
        .into_diagnostic()?;
    assert!(
        records.is_empty(),
        "deny(missing_docs) is enough: {records:?}"
    );
    Ok(())
}

#[test]
fn combined_forbid_list_satisfies_both() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_lib(fixture.path(), "#![forbid(unsafe_code, missing_docs)]\n")?;
    let records = scan_crate_attrs(fixture.path(), "fixture", &CrateAttrsThresholds::default())
        .into_diagnostic()?;
    assert!(
        records.is_empty(),
        "combined forbid list should cover both: {records:?}"
    );
    Ok(())
}

#[test]
fn cfg_attr_forbid_counts() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_lib(
        fixture.path(),
        "#![cfg_attr(not(test), forbid(unsafe_code))]\n#![warn(missing_docs)]\n",
    )?;
    let records = scan_crate_attrs(fixture.path(), "fixture", &CrateAttrsThresholds::default())
        .into_diagnostic()?;
    assert!(
        records.is_empty(),
        "crate-level cfg_attr forbid should count: {records:?}"
    );
    Ok(())
}

#[test]
fn bin_only_crate_is_skipped() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(fixture.path().join("src/main.rs"), "fn main() {}\n")
        .into_diagnostic()
        .wrap_err("write main")?;
    assert!(library_root_rs(fixture.path()).is_none());
    let records = scan_crate_attrs(fixture.path(), "fixture", &CrateAttrsThresholds::default())
        .into_diagnostic()?;
    assert!(records.is_empty(), "bin-only: {records:?}");
    Ok(())
}

#[test]
fn lib_path_override_is_the_root_not_src_lib_rs() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"moved\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/core.rs\"\n",
    )
    .into_diagnostic()
    .wrap_err("manifest")?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn decoy() {}\n")
        .into_diagnostic()
        .wrap_err("decoy lib")?;
    fs::write(fixture.path().join("src/core.rs"), BOTH)
        .into_diagnostic()
        .wrap_err("real lib")?;

    let root = library_root_rs(fixture.path()).expect("lib path");
    assert!(
        root.ends_with("src/core.rs"),
        "should honor [lib] path, got {}",
        root.display()
    );
    let records = scan_crate_attrs(fixture.path(), "moved", &CrateAttrsThresholds::default())
        .into_diagnostic()?;
    assert!(
        records.is_empty(),
        "attrs on [lib] path, not decoy src/lib.rs: {records:?}"
    );
    Ok(())
}

#[test]
fn lib_path_override_flags_the_named_file() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"moved\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/core.rs\"\n",
    )
    .into_diagnostic()
    .wrap_err("manifest")?;
    fs::write(fixture.path().join("src/lib.rs"), BOTH)
        .into_diagnostic()
        .wrap_err("decoy with attrs")?;
    fs::write(fixture.path().join("src/core.rs"), "pub fn real() {}\n")
        .into_diagnostic()
        .wrap_err("real lib")?;

    let records = scan_crate_attrs(fixture.path(), "moved", &CrateAttrsThresholds::default())
        .into_diagnostic()?;
    assert!(
        has(&records, CrateAttrsRuleId::ForbidUnsafe001),
        "must scan [lib] path, not src/lib.rs: {records:?}"
    );
    assert!(
        records.iter().all(|record| record
            .file
            .to_string_lossy()
            .replace('\\', "/")
            .ends_with("src/core.rs")),
        "finding file should be the [lib] path: {records:?}"
    );
    Ok(())
}

#[test]
fn allow_unsafe_skips_only_that_rule() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_lib(fixture.path(), "pub fn ffi() {}\n")?;
    let workspace = tempfile::tempdir()
        .into_diagnostic()
        .wrap_err("workspace")?;
    let store_home = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    fs::write(
        workspace.path().join("cordial.toml"),
        "[crate_attrs]\nallow_unsafe = [\"ffi\"]\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    let loaded = cordial::load_cordial_config(workspace.path(), store_home.path());
    let records =
        scan_crate_attrs(fixture.path(), "ffi", loaded.crate_attrs()).into_diagnostic()?;
    assert!(
        !has(&records, CrateAttrsRuleId::ForbidUnsafe001),
        "ffi is on allow_unsafe: {records:?}"
    );
    assert!(
        has(&records, CrateAttrsRuleId::MissingDocs001),
        "docs still required: {records:?}"
    );
    Ok(())
}

#[test]
fn toggling_a_rule_off_skips_it_everywhere() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_lib(fixture.path(), "pub fn ready() {}\n")?;
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
        .register(&CRATE_ATTRS_ETIQUETTE)
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
        .register(&CRATE_ATTRS_ETIQUETTE)
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
        .register(&CRATE_ATTRS_ETIQUETTE)
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
    eprintln!("{checklist}");
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
    assert_eq!(summary.inserted_attrs, 0);
    assert_eq!(summary.changed_files, 0);
    Ok(())
}

#[test]
fn apply_inserts_both_after_crate_docs() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_package(
        fixture.path(),
        "bare",
        "//! Crate docs.\n\npub fn ready() {}\n",
    )?;
    let summary =
        run_crate_attrs_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.inserted_attrs, 2);
    assert_eq!(summary.changed_files, 1);
    let lib = fs::read_to_string(fixture.path().join("src/lib.rs")).into_diagnostic()?;
    assert!(
        lib.contains("//! Crate docs."),
        "must keep crate docs: {lib}"
    );
    assert!(
        lib.contains("#![forbid(unsafe_code)]"),
        "must insert forbid: {lib}"
    );
    assert!(
        lib.contains("#![warn(missing_docs)]"),
        "must insert warn missing_docs: {lib}"
    );
    let docs_at = lib.find("//! Crate docs.").unwrap();
    let forbid_at = lib.find("#![forbid(unsafe_code)]").unwrap();
    let item_at = lib.find("pub fn ready()").unwrap();
    assert!(docs_at < forbid_at && forbid_at < item_at, "order: {lib}");
    Ok(())
}

#[test]
fn apply_is_idempotent_and_fills_only_the_gap() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_package(
        fixture.path(),
        "partial",
        "#![forbid(unsafe_code)]\n\npub fn ready() {}\n",
    )?;
    let first =
        run_crate_attrs_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(first.inserted_attrs, 1);
    let lib = fs::read_to_string(fixture.path().join("src/lib.rs")).into_diagnostic()?;
    assert_eq!(lib.matches("#![forbid(unsafe_code)]").count(), 1);
    assert!(lib.contains("#![warn(missing_docs)]"));
    let second =
        run_crate_attrs_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(second.changed_files, 0);
    assert_eq!(second.skipped_existing, 1);
    Ok(())
}

#[test]
fn apply_dry_run_does_not_write() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_package(fixture.path(), "bare", "pub fn ready() {}\n")?;
    let before = fs::read_to_string(fixture.path().join("src/lib.rs")).into_diagnostic()?;
    let summary =
        run_crate_attrs_apply(fixture.path(), fixture.path(), None, true).into_diagnostic()?;
    assert_eq!(summary.changed_files, 1);
    let after = fs::read_to_string(fixture.path().join("src/lib.rs")).into_diagnostic()?;
    assert_eq!(before, after);
    Ok(())
}

#[test]
fn apply_honors_allow_unsafe_and_lib_path() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"ffi\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"src/core.rs\"\n",
    )
    .into_diagnostic()
    .wrap_err("manifest")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[crate_attrs]\nallow_unsafe = [\"ffi\"]\n",
    )
    .into_diagnostic()
    .wrap_err("config")?;
    fs::write(fixture.path().join("src/lib.rs"), "pub fn decoy() {}\n")
        .into_diagnostic()
        .wrap_err("decoy")?;
    fs::write(fixture.path().join("src/core.rs"), "pub fn real() {}\n")
        .into_diagnostic()
        .wrap_err("core")?;

    let summary =
        run_crate_attrs_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.inserted_attrs, 1);
    let decoy = fs::read_to_string(fixture.path().join("src/lib.rs")).into_diagnostic()?;
    let core = fs::read_to_string(fixture.path().join("src/core.rs")).into_diagnostic()?;
    assert_eq!(decoy, "pub fn decoy() {}\n");
    assert!(
        !core.contains("#![forbid(unsafe_code)]"),
        "ffi is on allow_unsafe: {core}"
    );
    assert!(
        core.contains("#![warn(missing_docs)]"),
        "docs still applied on [lib] path: {core}"
    );
    Ok(())
}
