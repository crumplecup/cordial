//! A package on disk for the crate-attribute tests: a manifest and a library root.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

fn write_lib(root: &Path, body: &str) -> miette::Result<()> {
    fs::create_dir_all(root.join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(root.join("src/lib.rs"), body)
        .into_diagnostic()
        .wrap_err("write lib")?;
    Ok(())
}

pub fn write_package(root: &Path, name: &str, lib_body: &str) -> miette::Result<()> {
    fs::create_dir_all(root)
        .into_diagnostic()
        .wrap_err("root")?;
    fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
    )
    .into_diagnostic()
    .wrap_err("manifest")?;
    write_lib(root, lib_body)
}
