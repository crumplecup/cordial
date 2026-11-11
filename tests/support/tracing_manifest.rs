//! A minimal manifest for fixtures that rely on workspace-wide crate discovery.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

/// A minimal real `Cargo.toml` -- needed by any test whose fixture
/// relies on workspace-wide crate discovery via `cargo_metadata` (e.g.
/// `CallGraphFacts`'s own crate walk), which a bare directory with no
/// manifest at all is invisible to.
pub fn write_minimal_crate_manifest(root: &Path, crate_name: &str) -> miette::Result<()> {
    fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
    )
    .into_diagnostic()
    .wrap_err("crate manifest")
}
