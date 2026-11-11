//! `crate_attrs` apply: inserting the missing attributes without disturbing the rest of the file.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{CrateAttrsThresholds, run_crate_attrs_apply, scan_crate_attrs};

#[path = "support/crate_attrs_fixture.rs"]
mod crate_attrs_fixture;

use crate_attrs_fixture::write_package;
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
    assert_eq!(summary.inserted_attrs(), 2);
    assert_eq!(summary.changed_files(), 1);
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
    let docs_at = lib
        .find("//! Crate docs.")
        .ok_or_else(|| miette::miette!("expected crate docs in:\n{lib}"))?;
    let forbid_at = lib
        .find("#![forbid(unsafe_code)]")
        .ok_or_else(|| miette::miette!("expected forbid in:\n{lib}"))?;
    let item_at = lib
        .find("pub fn ready()")
        .ok_or_else(|| miette::miette!("expected item in:\n{lib}"))?;
    assert!(docs_at < forbid_at && forbid_at < item_at, "order: {lib}");
    Ok(())
}

#[test]
fn apply_respects_individual_attribute_switch_and_file_policy() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    write_package(fixture.path(), "bare", "pub fn ready() {}\n")?;
    fs::write(
        fixture.path().join("cordial.toml"),
        "[lints.crate_attrs.rules]\nCRATE-MISSING-DOCS-001 = false\n",
    )
    .into_diagnostic()?;
    let summary =
        run_crate_attrs_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.inserted_attrs(), 1);
    let lib = fixture.path().join("src/lib.rs");
    let source = fs::read_to_string(&lib).into_diagnostic()?;
    assert!(source.contains("#![forbid(unsafe_code)]"));
    assert!(!source.contains("missing_docs"));
    fs::write(
        fixture.path().join("cordial.toml"),
        "[[lints.crate_attrs.paths]]\npath = \"src/lib.rs\"\nenabled = false\n",
    )
    .into_diagnostic()?;
    let summary =
        run_crate_attrs_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(summary.inserted_attrs(), 0);
    assert_eq!(fs::read_to_string(&lib).into_diagnostic()?, source);
    Ok(())
}

#[test]
fn apply_handles_library_entry_outside_src() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = \"custom_root\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[lib]\npath = \"library.rs\"\n",
    )
    .into_diagnostic()?;
    let path = fixture.path().join("library.rs");
    fs::write(&path, "//! Custom root.\n\npub fn ready() {}\n").into_diagnostic()?;
    let before = scan_crate_attrs(
        fixture.path(),
        "custom_root",
        &CrateAttrsThresholds::default(),
    )
    .into_diagnostic()?;
    assert_eq!(before.len(), 2);
    assert!(
        before
            .iter()
            .all(|record| record.file() == Path::new("library.rs"))
    );
    let applied =
        run_crate_attrs_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(applied.changed_files(), 1);
    assert_eq!(applied.inserted_attrs(), 2);
    assert_eq!(
        fs::read_to_string(&path).into_diagnostic()?,
        "//! Custom root.\n\n#![forbid(unsafe_code)]\n#![warn(missing_docs)]\npub fn ready() {}\n"
    );
    assert!(
        scan_crate_attrs(
            fixture.path(),
            "custom_root",
            &CrateAttrsThresholds::default()
        )
        .into_diagnostic()?
        .is_empty()
    );
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
    assert_eq!(first.inserted_attrs(), 1);
    let lib = fs::read_to_string(fixture.path().join("src/lib.rs")).into_diagnostic()?;
    assert_eq!(lib.matches("#![forbid(unsafe_code)]").count(), 1);
    assert!(lib.contains("#![warn(missing_docs)]"));
    let second =
        run_crate_attrs_apply(fixture.path(), fixture.path(), None, false).into_diagnostic()?;
    assert_eq!(second.changed_files(), 0);
    assert_eq!(second.skipped_existing(), 1);
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
    assert_eq!(summary.changed_files(), 1);
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
    assert_eq!(summary.inserted_attrs(), 1);
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
