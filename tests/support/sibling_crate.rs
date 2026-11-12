//! A sibling crate a fixture depends on, so calls into it resolve to a definition.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;
use std::path::Path;

/// Add a path dependency `other_crate` to the fixture at `root`, whose source is `source`.
pub fn add_other_crate(root: &Path, source: &str) -> miette::Result<()> {
    let manifest = root.join("Cargo.toml");
    let text = fs::read_to_string(&manifest).into_diagnostic()?;
    fs::write(
        &manifest,
        text.replace(
            "[workspace]\n",
            "other_crate = { path = 'other_crate' }\n[workspace]\n",
        ),
    )
    .into_diagnostic()?;
    fs::create_dir_all(root.join("other_crate/src"))
        .into_diagnostic()
        .wrap_err("other_crate dir")?;
    fs::write(
        root.join("other_crate/Cargo.toml"),
        "[package]\nname = 'other_crate'\nversion = '0.1.0'\nedition = '2024'\n",
    )
    .into_diagnostic()?;
    fs::write(root.join("other_crate/src/lib.rs"), source).into_diagnostic()
}
