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
    // A `verus!` block is read only when its path resolves to the Verus macro, so
    // a source that imports it depends on the crate that defines it.
    let dependencies = if source.contains("verus_builtin_macros") {
        "[dependencies]\nverus_builtin_macros = '=0.0.0-2026-11-01-0251'\n"
    } else {
        ""
    };
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n{dependencies}[workspace]\n"
        ),
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/lib.rs"), source).into_diagnostic()?;
    source_projection::baseline(root, &cordial::CrateTarget::new("fixture", root))
}
