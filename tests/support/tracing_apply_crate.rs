//! A one-crate workspace on disk with a source file and a checklist, for the tracing apply tests.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::PathBuf;

use crate::tracing_apply_checklist::checklist_for_crate;

pub struct ApplyFixture {
    pub workspace: PathBuf,
    pub src: PathBuf,
    pub checklist: PathBuf,
    pub _temp: tempfile::TempDir,
}

pub fn write_apply_fixture(source: &str, checklist: &str) -> miette::Result<ApplyFixture> {
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

pub fn checklist_for(items: &[(&str, u32)]) -> String {
    checklist_for_crate("fixture_crate", items)
}
