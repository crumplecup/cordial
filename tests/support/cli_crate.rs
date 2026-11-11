//! A minimal library crate on disk for the CLI tests to run against.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

pub fn write_minimal_crate(root: &std::path::Path, lib_rs: &str) -> miette::Result<()> {
    fs::create_dir_all(root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(root.join("src/lib.rs"), lib_rs)
        .into_diagnostic()
        .wrap_err("write fixture")?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .into_diagnostic()
    .wrap_err("write manifest")?;
    Ok(())
}
