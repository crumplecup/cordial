//! What the crate-attribute scan accepts and flags: forbid/deny forms, library-root overrides, and the Verus exemption.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

use cordial::{
    CRATE_ATTRS_ETIQUETTE, CrateAttrsRuleId, CrateAttrsThresholds, RunAll, Session, SessionBuilder,
    library_root_rs, scan_crate_attrs,
};

#[path = "support/crate_attrs_fixture.rs"]
mod crate_attrs_fixture;

use crate_attrs_fixture::write_package;
const BOTH: &str = r#"
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub fn ready() {}
"#;

fn has(records: &[cordial::CrateAttrsSiteRecord], rule: CrateAttrsRuleId) -> bool {
    records.iter().any(|record| record.rule_id() == rule)
}

#[test]
fn scan_flags_both_when_library_root_is_bare() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_package(fixture.path(), "fixture", "pub fn ready() {}\n")?;
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
    write_package(fixture.path(), "fixture", BOTH)?;
    let records = scan_crate_attrs(fixture.path(), "fixture", &CrateAttrsThresholds::default())
        .into_diagnostic()?;
    assert!(records.is_empty(), "compliant lib: {records:?}");
    Ok(())
}

#[test]
fn deny_unsafe_code_is_not_forbid() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    write_package(
        fixture.path(),
        "fixture",
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
    write_package(
        fixture.path(),
        "fixture",
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
    write_package(
        fixture.path(),
        "fixture",
        "#![forbid(unsafe_code, missing_docs)]\n",
    )?;
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
    write_package(
        fixture.path(),
        "fixture",
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

    let root = library_root_rs(fixture.path()).ok_or_else(|| miette::miette!("lib path"))?;
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
            .file()
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
    write_package(fixture.path(), "ffi", "pub fn ffi() {}\n")?;
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

/// A verus-target crate (directory name ending `_verus`) that states a
/// real `assume_specification` claim is exempt from
/// `CRATE-FORBID-UNSAFE-001`: the real `verus` binary expands
/// `assume_specification` into constructs its own `forbid(unsafe_code)`
/// genuinely rejects, confirmed against a real `verus --crate-type=lib`
/// invocation (see `crates/amenable_verus/src/lib.rs`'s own doc comment
/// for the amenable-side story) -- invisible to plain `cargo build`/
/// clippy, which never see this.
#[test]
fn verus_target_with_assume_specification_skips_forbid_unsafe() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("fixture_verus");
    write_package(&crate_root, "fixture_verus", "pub fn ready() {}\n")?;
    fs::write(
        crate_root.join("src").join("carrier.rs"),
        "verus! { pub assume_specification [char::from_str] (s: &str) -> (result: Result<char, ()>); }\n",
    )
    .into_diagnostic()
    .wrap_err("write carrier")?;
    let records = scan_crate_attrs(
        &crate_root,
        "fixture_verus",
        &CrateAttrsThresholds::default(),
    )
    .into_diagnostic()?;
    assert_eq!(
        !has(&records, CrateAttrsRuleId::ForbidUnsafe001),
        cfg!(feature = "verus_warnings"),
        "Verus exemption requires the Verus integration feature: {records:?}"
    );
    assert!(
        has(&records, CrateAttrsRuleId::MissingDocs001),
        "the exemption is narrow -- missing_docs still applies: {records:?}"
    );
    Ok(())
}

#[test]
fn session_uses_loaded_verus_source_for_crate_attrs_exemption() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("fixture_verus");
    fs::create_dir_all(&crate_root).into_diagnostic()?;
    write_package(&crate_root, "fixture_verus", "pub fn ready() {}\n")?;
    fs::write(
        crate_root.join("src/carrier.rs"),
        "verus! { pub assume_specification [char::from_str] (s: &str) -> (result: Result<char, ()>); }\n",
    )
    .into_diagnostic()?;
    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(&crate_root)
        .with_store_root(store.path())
        .register(&*CRATE_ATTRS_ETIQUETTE)
        .build();
    let outcome = session.run(&RunAll).into_diagnostic()?;
    let ids: Vec<_> = outcome
        .findings()
        .map(|finding| finding.rule().id())
        .collect();
    assert_eq!(
        !ids.contains(&"CRATE-FORBID-UNSAFE-001"),
        cfg!(feature = "verus_warnings"),
        "Verus exemption requires the Verus integration feature: {ids:?}"
    );
    assert!(ids.contains(&"CRATE-MISSING-DOCS-001"), "{ids:?}");
    Ok(())
}

/// Both signals are required: a verus-target crate that never actually
/// states an `assume_specification` claim has no reason to be exempt.
#[test]
fn verus_target_without_assume_specification_still_flags_forbid_unsafe() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("fixture_verus");
    write_package(&crate_root, "fixture_verus", "pub fn ready() {}\n")?;
    let records = scan_crate_attrs(
        &crate_root,
        "fixture_verus",
        &CrateAttrsThresholds::default(),
    )
    .into_diagnostic()?;
    assert!(
        has(&records, CrateAttrsRuleId::ForbidUnsafe001),
        "no real assume_specification site means no exemption: {records:?}"
    );
    Ok(())
}

/// Same converse: a non-verus-target crate that happens to mention
/// `assume_specification` in a comment or string literal has no reason
/// to be exempt either.
#[test]
fn non_verus_crate_with_assume_specification_text_still_flags_forbid_unsafe() -> miette::Result<()>
{
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = fixture.path().join("fixture_plain");
    write_package(
        &crate_root,
        "fixture_plain",
        "// mentions assume_specification but isn't a verus target\npub fn ready() {}\n",
    )?;
    let records = scan_crate_attrs(
        &crate_root,
        "fixture_plain",
        &CrateAttrsThresholds::default(),
    )
    .into_diagnostic()?;
    assert!(
        has(&records, CrateAttrsRuleId::ForbidUnsafe001),
        "not a verus-target crate, so no exemption: {records:?}"
    );
    Ok(())
}
