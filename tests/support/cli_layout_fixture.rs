//! A CLI crate on disk (lib plus main) and the layout records the loader derives for it.

use cordial::{CliLayoutRecord, select_cli_layout_records};
use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

#[path = "policy_inventory.rs"]
mod policy_inventory;

/// The layout findings the loader's CLI catalog gives the crate at `root`.
pub fn cli_layout_records(root: &Path, crate_name: &str) -> miette::Result<Vec<CliLayoutRecord>> {
    let inventory = policy_inventory::policy_inventory(root)?;
    select_cli_layout_records(inventory.cli_layout(), crate_name).into_diagnostic()
}

pub fn write_cli_crate(lib: &str, main: &str) -> miette::Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    fs::create_dir_all(fixture.path().join("src"))
        .into_diagnostic()
        .wrap_err("src dir")?;
    fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    fs::write(fixture.path().join("src/lib.rs"), lib)
        .into_diagnostic()
        .wrap_err("write lib")?;
    fs::write(fixture.path().join("src/main.rs"), main)
        .into_diagnostic()
        .wrap_err("write main")?;
    Ok(fixture)
}
