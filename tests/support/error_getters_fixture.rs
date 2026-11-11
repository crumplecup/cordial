//! The error-getters fixture crate and the helpers that apply, configure and compile it.
#![cfg(all(feature = "internal_error_chain", feature = "cli", feature = "quality"))]

use miette::IntoDiagnostic;
use std::fs;
use std::path::Path;
use std::process::Command;

pub const ORIGINAL: &str = r#"use derive_getters::Getters;
use derive_more::{Display, Error};
/// A located failure.
#[derive(Debug, Display, Error, Getters)]
#[display("failure at {file}:{line}")]
pub struct Failure {
    /// Source file.
    file: String,
    line: u32,
}
impl Failure {
    #[track_caller]
    pub fn new() -> Self {
        let loc = std::panic::Location::caller();
        Self { file: loc.file().to_string(), line: loc.line() }
    }
}
"#;

pub fn fixture() -> miette::Result<tempfile::TempDir> {
    let root = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(root.path().join("src")).into_diagnostic()?;
    fs::create_dir_all(root.path().join("tests")).into_diagnostic()?;
    fs::create_dir_all(root.path().join(".cargo")).into_diagnostic()?;
    fs::write(
        root.path().join(".cargo/config.toml"),
        "[net]\noffline = true\n",
    )
    .into_diagnostic()?;
    fs::write(
        root.path().join("Cargo.toml"),
        r#"[package]
name = "getter-apply"
version = "0.0.0"
edition = "2024"
[dependencies]
derive-getters = "=0.5.0"
derive_more = { version = "2", features = ["display", "error"] }
[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ["cfg(kani)"] }
"#,
    )
    .into_diagnostic()?;
    fs::write(root.path().join("src/lib.rs"), ORIGINAL).into_diagnostic()?;
    fs::write(
        root.path().join("tests/behavior.rs"),
        r#"#[test]
fn local_error_getters_preserve_caller() {
    let expected = line!() + 1;
    let error = getter_apply::Failure::new();
    assert_eq!(*error.line(), expected);
    assert_eq!(error.file(), file!());
    assert_eq!(error.to_string(), format!("failure at {}:{expected}", file!()));
}
"#,
    )
    .into_diagnostic()?;
    configure(root.path(), "borrowed")?;
    Ok(root)
}

pub fn configure(root: &Path, strategy: &str) -> miette::Result<()> {
    fs::write(
        root.join("cordial.toml"),
        format!("[internal_error_chain]\nfield_strategy = \"{strategy}\"\n"),
    )
    .into_diagnostic()
}

pub fn check(root: &Path) -> miette::Result<()> {
    let output = Command::new("cargo")
        .args(["test", "--offline", "--quiet"])
        .current_dir(root)
        .output()
        .into_diagnostic()?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
