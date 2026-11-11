//! Writes a throwaway crate for the error-chain tests.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

pub fn write_error_crate(src: &str) -> miette::Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/error.rs"), src)
        .into_diagnostic()
        .wrap_err("write error.rs")?;
    fs::write(fixture.path().join("src/lib.rs"), "mod error;\n")
        .into_diagnostic()
        .wrap_err("write lib")?;
    Ok(fixture)
}
