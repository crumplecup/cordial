//! Throwaway one-file crates projected through the database.
use cordial::CrateIr;
use miette::{IntoDiagnostic, Result};

#[path = "source_projection.rs"]
mod source_projection;

/// Place `source` as the lib of a throwaway crate and project it through the
/// database, so a test reads exactly what the loader would store.
#[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
pub fn project_source(source: &str) -> Result<CrateIr> {
    let fixture = tempfile::tempdir().into_diagnostic()?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/lib.rs"), source).into_diagnostic()?;
    source_projection::baseline(root, &cordial::CrateTarget::new("fixture", root))
}
