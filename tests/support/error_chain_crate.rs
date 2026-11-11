//! A throwaway crate whose lib is the given source, optionally a proc-macro lib.

use miette::IntoDiagnostic;
use std::fs;

pub fn crate_with_lib(source: &str, proc_macro: bool) -> miette::Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic()?;
    fs::create_dir_all(fixture.path().join("src")).into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), source).into_diagnostic()?;
    let lib = if proc_macro {
        "[lib]\nproc-macro = true\n"
    } else {
        ""
    };
    fs::write(
        fixture.path().join("Cargo.toml"),
        format!(
            "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n{lib}[workspace]\n"
        ),
    )
    .into_diagnostic()?;
    Ok(fixture)
}
