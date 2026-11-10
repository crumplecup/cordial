//! Panic sites of one source text, read from the database-backed inventory.
use cordial::{CrateTarget, IrView, PanicSiteRecord, SourceDatabase};
use miette::{IntoDiagnostic, Result, WrapErr};

/// Place `source` as the lib of a throwaway crate, load it, and return the
/// panic records the loader projects for it.
#[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
pub fn panic_sites(source: &str) -> Result<Vec<PanicSiteRecord>> {
    let fixture = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let root = fixture.path();
    std::fs::create_dir_all(root.join("src")).into_diagnostic()?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = 'fixture'\nversion = '0.1.0'\nedition = '2024'\n[workspace]\n",
    )
    .into_diagnostic()?;
    std::fs::write(root.join("src/lib.rs"), source).into_diagnostic()?;
    let ir = SourceDatabase::load(root)
        .into_diagnostic()?
        .project_target(&CrateTarget::new("fixture", root))
        .into_diagnostic()?;
    Ok(ir
        .source_panics()
        .into_diagnostic()
        .wrap_err("panic inventory")?
        .records()
        .clone())
}
