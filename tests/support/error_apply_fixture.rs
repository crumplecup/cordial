//! A throwaway crate for the error-apply tests: write the source, set the field strategy, read the
//! rewritten source back, and check that it still compiles.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn fixture(source: &str, strategy: &str) -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"file-apply-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .into_diagnostic()?;
    fs::write(root.path().join("src/lib.rs"), source).into_diagnostic()?;
    set_strategy(root.path(), strategy)?;
    Ok(root)
}

pub fn set_strategy(root: &Path, strategy: &str) -> miette::Result<()> {
    fs::write(
        root.join("cordial.toml"),
        format!("[internal_error_chain]\nfield_strategy = \"{strategy}\"\n"),
    )
    .into_diagnostic()
    .wrap_err("write strategy")
}

pub fn source(root: &Path) -> miette::Result<String> {
    fs::read_to_string(root.join("src/lib.rs")).into_diagnostic()
}

pub fn check_fixture(root: &Path) -> miette::Result<()> {
    let result = Command::new("cargo")
        .args(["check", "--offline", "--quiet"])
        .current_dir(root)
        .output()
        .into_diagnostic()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(())
}
