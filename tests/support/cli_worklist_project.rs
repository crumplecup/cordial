//! Runs the `cordial` binary against a fixture project with its own store home.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

pub fn run(root: &Path, home: &Path, args: &[&str]) -> miette::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(root)
        .arg("--store-home")
        .arg(home)
        .args(args)
        .env("CORDIAL_NO_PROGRESS", "1")
        .output()
        .into_diagnostic()
        .wrap_err_with(|| format!("cordial {}", args.join(" ")))
}

pub fn fixture() -> miette::Result<(tempfile::TempDir, tempfile::TempDir, cordial::StoreLayout)> {
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(project.path().join("src")).into_diagnostic()?;
    fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"worklist-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(
        project.path().join("src/lib.rs"),
        include_str!("../fixtures/panics/cli_quality.rs"),
    )
    .into_diagnostic()?;
    let slug = cordial::project_slug_from_path(project.path());
    let store = cordial::StoreLayout::from_root(home.path().join(&slug), slug);
    Ok((project, home, store))
}
