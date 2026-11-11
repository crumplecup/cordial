//! The recipe apply writes for each role: plain, getter, `err`, entry point, and result aliases.

use cordial::{parse_tracing_instrument_checklist_text, run_tracing_instrument_apply};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

#[path = "support/tracing_apply_checklist.rs"]
mod tracing_apply_checklist;

#[path = "support/tracing_apply_crate.rs"]
mod tracing_apply_crate;

use tracing_apply_crate::{checklist_for, write_apply_fixture};

#[test]
fn parse_tracing_checklist_groups_by_crate_section() -> miette::Result<()> {
    cordial::init_tracing();
    let body = fs::read_to_string("tests/fixtures/quality/apply_checklist.md")
        .into_diagnostic()
        .wrap_err("read apply checklist fixture")?;
    let gaps = parse_tracing_instrument_checklist_text(&body)
        .into_diagnostic()
        .wrap_err("parse checklist")?;
    assert_eq!(gaps.len(), 2);
    assert_eq!(gaps[0].crate_name(), "fixture_crate");
    assert_eq!(gaps[0].qualified_name(), "apply_target::missing");
    assert_eq!(gaps[0].rel_path(), Path::new("src/lib.rs"));
    assert_eq!(gaps[0].line(), 1);
    Ok(())
}

#[test]
fn apply_inserts_instrument_from_checklist() -> miette::Result<()> {
    cordial::init_tracing();
    let source = fs::read_to_string("tests/fixtures/quality/apply_target/src/lib.rs")
        .into_diagnostic()
        .wrap_err("read apply target")?;
    let checklist = fs::read_to_string("tests/fixtures/quality/apply_checklist.md")
        .into_diagnostic()
        .wrap_err("read apply checklist")?;
    let fixture = write_apply_fixture(&source, &checklist)?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;

    assert_eq!(summary.changed_functions(), 2);
    assert_eq!(summary.changed_files(), 1);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(updated.contains("#[instrument(level = \"debug\", skip(path, report))]"));
    assert!(updated.contains("#[instrument(level = \"debug\", skip(path))]"));
    assert!(updated.contains("use tracing::instrument;"));
    Ok(())
}

#[test]
fn apply_writes_getter_trace_recipe() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub struct Store {
    root: String,
}

impl Store {
    pub fn as_str(&self) -> &str {
        &self.root
    }
}
"#,
        &checklist_for(&[("Store::as_str", 7)]),
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"trace\", skip(self))]"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_writes_io_err_recipe() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub fn load_session_config() -> Result<(), String> {
    Ok(())
}
"#,
        &checklist_for(&[("load_session_config", 2)]),
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions(), 1);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"info\", err(level = \"warn\"))]"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_writes_entry_recipe_for_run_prefix() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub fn run_apply_patches() {}
"#,
        &checklist_for(&[("run_apply_patches", 2)]),
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
        updated.contains("#[instrument(level = \"info\")]"),
        "{updated}"
    );
    Ok(())
}

#[test]
fn apply_does_not_treat_free_path_fn_as_getter() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub fn trait_impls_for_path(key: &str) -> Option<u8> {
    None
}
"#,
        &checklist_for(&[("trait_impls_for_path", 2)]),
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
        "free *_path is Other at debug, not a trace getter: {updated}"
    );
    assert!(!updated.contains("level = \"trace\""), "{updated}");
    Ok(())
}

#[test]
fn apply_writes_err_for_result_alias() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = write_apply_fixture(
        r#"
pub type CordialResult<T> = Result<T, String>;

pub fn load_session_config() -> CordialResult<()> {
    Ok(())
}
"#,
        &checklist_for(&[("load_session_config", 4)]),
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
        updated.contains("#[instrument(level = \"info\", err(level = \"warn\"))]"),
        "{updated}"
    );
    Ok(())
}
