use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

use cordial::{parse_tracing_instrument_checklist_text, run_tracing_instrument_apply};

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
    let temp = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let workspace = temp.path().join("workspace");
    let crate_root = workspace.join("fixture_crate");
    let src_root = crate_root.join("src");
    fs::create_dir_all(&src_root)
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::copy(
        "tests/fixtures/quality/apply_target/src/lib.rs",
        src_root.join("lib.rs"),
    )
    .into_diagnostic()
    .wrap_err("copy fixture")?;

    let checklist = temp.path().join("tracing-instrument.checklist.md");
    fs::copy("tests/fixtures/quality/apply_checklist.md", &checklist)
        .into_diagnostic()
        .wrap_err("copy checklist")?;

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

    let summary =
        run_tracing_instrument_apply(&workspace, &checklist, Some("fixture_crate"), false)
            .into_diagnostic()
            .wrap_err("apply tracing")?;

    assert_eq!(summary.changed_functions, 2);
    assert_eq!(summary.changed_files, 1);

    let updated = fs::read_to_string(src_root.join("lib.rs"))
        .into_diagnostic()
        .wrap_err("read updated source")?;
    assert!(updated.contains("#[instrument(skip(path, report))]"));
    assert!(updated.contains("#[instrument(skip(path))]"));
    assert!(updated.contains("use tracing::instrument;"));
    Ok(())
}
