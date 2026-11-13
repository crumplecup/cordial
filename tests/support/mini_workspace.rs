//! A two-file package with a call across them, for tests that edit it.

use miette::{IntoDiagnostic, Result};
use std::path::Path;

/// Write the package at `root`: `top` in `lib.rs` calls `leaf` in `other.rs`.
#[tracing::instrument(level = "debug", err(level = "warn"))]
pub(crate) fn write(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    for (path, text) in [
        (
            "Cargo.toml",
            "[package]\nname = 'mini'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
        ),
        (
            "src/lib.rs",
            "mod other;\npub fn top() -> u8 {\n    other::leaf() + 1\n}\n",
        ),
        ("src/other.rs", "pub fn leaf() -> u8 {\n    1\n}\n"),
    ] {
        std::fs::write(root.join(path), text).into_diagnostic()?;
    }
    Ok(())
}
