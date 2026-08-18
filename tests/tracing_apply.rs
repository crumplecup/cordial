use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::{Path, PathBuf};

use cordial::{parse_tracing_instrument_checklist_text, run_tracing_instrument_apply};

struct ApplyFixture {
    workspace: PathBuf,
    src: PathBuf,
    checklist: PathBuf,
    _temp: tempfile::TempDir,
}

fn write_apply_fixture(source: &str, checklist: &str) -> miette::Result<ApplyFixture> {
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let workspace = temp.path().join("workspace");
    let crate_root = workspace.join("fixture_crate");
    let src_root = crate_root.join("src");
    fs::create_dir_all(&src_root)
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(src_root.join("lib.rs"), source)
        .into_diagnostic()
        .wrap_err("write source")?;
    fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"fixture_crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()
    .wrap_err("crate manifest")?;
    fs::write(
        workspace.join("Cargo.toml"),
        "[workspace]\nmembers = [\"fixture_crate\"]\nresolver = \"2\"\n",
    )
    .into_diagnostic()
    .wrap_err("workspace manifest")?;
    let checklist_path = temp.path().join("tracing-instrument.checklist.md");
    fs::write(&checklist_path, checklist)
        .into_diagnostic()
        .wrap_err("write checklist")?;
    Ok(ApplyFixture {
        workspace,
        src: src_root.join("lib.rs"),
        checklist: checklist_path,
        _temp: temp,
    })
}

fn checklist_for(items: &[(&str, u32)]) -> String {
    let mut body = String::from(
        "# Tracing instrument checklist\n\n**Open gaps:** n\n\n## `fixture_crate`\n\n",
    );
    for (qualified_name, line) in items {
        body.push_str(&format!(
            "- [ ] `{qualified_name}` — `src/lib.rs:{line}` (pub)\n"
        ));
    }
    body
}

#[test]
fn parse_tracing_checklist_groups_by_crate_section() -> miette::Result<()> {
    let body = fs::read_to_string("tests/fixtures/quality/apply_checklist.md")
        .into_diagnostic()
        .wrap_err("read apply checklist fixture")?;
    let gaps = parse_tracing_instrument_checklist_text(&body);
    assert_eq!(gaps.len(), 2);
    assert_eq!(gaps[0].crate_name, "fixture_crate");
    assert_eq!(gaps[0].qualified_name, "apply_target::missing");
    assert_eq!(gaps[0].rel_path, Path::new("src/lib.rs"));
    assert_eq!(gaps[0].line, 1);
    Ok(())
}

#[test]
fn apply_inserts_instrument_from_checklist() -> miette::Result<()> {
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

    assert_eq!(summary.changed_functions, 2);
    assert_eq!(summary.changed_files, 1);

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
    assert_eq!(summary.changed_functions, 1);

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
    assert_eq!(summary.changed_functions, 1);

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
fn apply_rewrites_existing_instrument_to_recipe() -> miette::Result<()> {
    let fixture = write_apply_fixture(
        r#"
pub struct Store {
    root: String,
}

impl Store {
    #[tracing::instrument]
    pub fn cache_dir(&self) -> String {
        format!("{}/cache", self.root)
    }
}
"#,
        &checklist_for(&[("Store::cache_dir", 8)]),
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1);
    assert_eq!(summary.skipped_existing, 0);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(
        updated.contains("#[instrument(level = \"trace\", skip(self))]"),
        "{updated}"
    );
    assert!(
        !updated.contains("#[tracing::instrument]"),
        "bare default-info attr should be replaced: {updated}"
    );
    assert!(updated.contains("use tracing::instrument;"));
    Ok(())
}

#[test]
fn apply_skips_when_recipe_already_present() -> miette::Result<()> {
    let fixture = write_apply_fixture(
        r#"
use tracing::instrument;

#[instrument(level = "info", err(level = "warn"))]
pub fn load_session_config() -> Result<(), String> {
    Ok(())
}
"#,
        &checklist_for(&[("load_session_config", 5)]),
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 0);
    assert_eq!(summary.skipped_existing, 1);
    Ok(())
}

#[test]
fn apply_dedupes_multiple_checklist_rows_for_one_fn() -> miette::Result<()> {
    let mut checklist = checklist_for(&[("load_session_config", 2)]);
    checklist.push_str("- [ ] `load_session_config` — `src/lib.rs:2` (pub)\n");
    let fixture = write_apply_fixture(
        r#"
pub fn load_session_config() -> Result<(), String> {
    Ok(())
}
"#,
        &checklist,
    )?;

    let summary = run_tracing_instrument_apply(
        &fixture.workspace,
        &fixture.checklist,
        Some("fixture_crate"),
        false,
    )
    .into_diagnostic()
    .wrap_err("apply tracing")?;
    assert_eq!(summary.changed_functions, 1);

    let updated = fs::read_to_string(&fixture.src)
        .into_diagnostic()
        .wrap_err("read updated source")?;
    let count = updated.matches("#[instrument(").count();
    assert_eq!(count, 1, "{updated}");
    Ok(())
}
