//! Where apply puts the attribute and which import form it uses.

use cordial::run_tracing_instrument_apply;
use miette::{IntoDiagnostic, WrapErr};
use std::fs;

#[path = "support/tracing_apply_checklist.rs"]
mod tracing_apply_checklist;

#[path = "support/tracing_apply_crate.rs"]
mod tracing_apply_crate;

use tracing_apply_crate::{checklist_for, write_apply_fixture};

#[test]
fn apply_does_not_split_doc_from_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
/// A documented helper.
pub fn scan_tree() {}
"#,
        &checklist_for(&[("scan_tree", 3)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        !updated.contains("/// A documented helper.\nuse tracing::instrument;"),
        "use must not land between docs and the item:\n{updated}"
    );
    let use_idx = updated
        .find("use tracing::instrument;")
        .ok_or_else(|| miette::miette!("expected `use tracing::instrument;` in:\n{updated}"))?;
    let doc_idx = updated
        .find("/// A documented helper.")
        .ok_or_else(|| miette::miette!("expected doc comment in:\n{updated}"))?;
    assert!(use_idx < doc_idx, "{updated}");
    Ok(())
}

#[test]
fn apply_does_not_put_err_on_option() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub fn scan_first() -> Option<u8> {
    let value = std::fs::read_to_string("x").ok()?;
    value.bytes().next()
}
"#,
        &checklist_for(&[("scan_first", 2)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"debug\")]"),
        "{updated}"
    );
    assert!(!updated.contains("err("), "{updated}");
    Ok(())
}

#[test]
fn apply_matches_fn_name_not_nearest_fn() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub fn fmt() {}

pub fn scan_tree() {}
"#,
        &checklist_for(&[("scan_tree", 2)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("pub fn fmt() {}"),
        "stale checklist line must not stamp the nearer fn:\n{updated}"
    );
    assert!(
        updated.contains("#[instrument(level = \"debug\")]\npub fn scan_tree()"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_uses_crate_path_when_tracing_is_a_module() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
mod tracing;

pub fn scan_tree() {}
"#,
        &checklist_for(&[("scan_tree", 4)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[::tracing::instrument(level = \"debug\")]"),
        "{updated}"
    );
    assert!(!updated.contains("use tracing::instrument;"), "{updated}");
    Ok(())
}

#[test]
fn apply_does_not_split_derive_from_item() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
#[derive(Debug)]
pub enum Boom {
    A,
}

pub fn load_config() -> Result<(), Boom> {
    Ok(())
}
"#,
        &checklist_for(&[("load_config", 7)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(updated.contains("use tracing::instrument;"), "{updated}");
    assert!(
        !updated.contains("#[derive(Debug)]\nuse tracing::instrument;"),
        "use must not land between derive and the item:\n{updated}"
    );
    let use_idx = updated
        .find("use tracing::instrument;")
        .ok_or_else(|| miette::miette!("expected `use tracing::instrument;` in:\n{updated}"))?;
    let derive_idx = updated
        .find("#[derive(Debug)]")
        .ok_or_else(|| miette::miette!("expected `#[derive(Debug)]` in:\n{updated}"))?;
    assert!(use_idx < derive_idx, "{updated}");
    Ok(())
}

#[test]
fn apply_skips_impl_trait_params() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub struct Artifact;

impl Artifact {
    pub fn new(crate_name: impl Into<String>) -> Self {
        let _ = crate_name.into();
        Self
    }
}
"#,
        &checklist_for(&[("Artifact::new", 6)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"debug\", skip(crate_name))]"),
        "{updated}"
    );
    assert!(
        !updated.contains("fields(crate_name"),
        "impl Trait identity args are unrecordable: {updated}"
    );
    Ok(())
}

#[test]
fn apply_does_not_duplicate_grouped_tracing_import() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
use tracing::{debug, instrument};

pub fn scan_tree() {
    debug!("scan");
}
"#,
        &checklist_for(&[("scan_tree", 4)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert_eq!(updated.matches("use tracing::").count(), 1, "{updated}");
    assert!(
        updated.contains("#[instrument(level = \"debug\")]"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_uses_path_form_when_instrument_is_a_module() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
mod instrument;

pub fn scan_tree() {}
"#,
        &checklist_for(&[("scan_tree", 4)]),
    )?;

    run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[tracing::instrument(level = \"debug\")]"),
        "{updated}"
    );
    assert!(!updated.contains("use tracing::instrument;"), "{updated}");
    Ok(())
}
