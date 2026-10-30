//! Write only a closed plan whose complete proof inputs are still current.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;

use super::ClosedPanicMigration;
use crate::error::{CordialError, CordialResult};
use crate::source_write::{SourceWrite, write_sources};

/// Planned and actual effects of one closed panic migration.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct PanicMigrationApplySummary {
    /// Files in the accepted transformation unit, including a dry run.
    #[getter(copy)]
    planned_files: usize,
    /// Files actually replaced; zero during dry run.
    #[getter(copy)]
    written_files: usize,
    /// Proven panic sites replaced with typed propagation.
    #[getter(copy)]
    converted_sites: usize,
    /// Function contracts adapted within the component.
    #[getter(copy)]
    changed_functions: usize,
}

/// Apply one proven migration, or validate it without writing during dry run.
///
/// The closed token is required: isolated previews cannot reach this writer.
/// Source bytes are read only to validate snapshots and render located edits.
/// Every proof input, including untouched source and the Cargo manifest, must
/// still match; newly added or removed Rust files invalidate the inventory.
/// This library operation does not claim worklist items on its own.
#[tracing::instrument(level = "debug", skip(migration), err(level = "warn"))]
#[hotpath::measure]
pub fn apply_closed_panic_migration(
    migration: &ClosedPanicMigration,
    dry_run: bool,
) -> CordialResult<PanicMigrationApplySummary> {
    validate_inputs(migration)?;
    let plan = migration.component().plan();
    let source = fs::read_to_string(plan.path())?;
    let replacement = plan.preview(&source)?;
    let write = SourceWrite::new(plan.path().clone(), source, replacement);
    write_sources(&[write], dry_run, || validate_inputs(migration))?;
    let changed_functions = migration.component().functions().len();
    let converted_sites = migration.component().sites().len();
    tracing::info!(path = %plan.path().display(), dry_run, changed_functions, converted_sites,
        "closed panic migration validated");
    Ok(PanicMigrationApplySummary {
        planned_files: 1,
        written_files: usize::from(!dry_run),
        converted_sites,
        changed_functions,
    })
}

#[tracing::instrument(level = "trace", skip(migration), err(level = "warn"))]
fn validate_inputs(migration: &ClosedPanicMigration) -> CordialResult<()> {
    for input in migration.inputs() {
        let bytes = fs::read(input.path())?;
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        if digest != input.digest() {
            return Err(CordialError::stale_source_snapshot(input.path()));
        }
    }
    // Directory membership is freshness evidence, not another source analysis.
    // It detects new uses omitted by an earlier, otherwise unchanged inventory.
    let mut current = BTreeSet::new();
    for entry in walkdir::WalkDir::new(migration.source_root()) {
        let entry = entry.map_err(|error| std::io::Error::other(error))?;
        if entry.file_type().is_symlink() {
            return Err(std::io::Error::from(std::io::ErrorKind::Unsupported).into());
        }
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "rs") {
            current.insert(entry.path().to_path_buf());
        }
    }
    let expected = migration
        .inputs()
        .iter()
        .filter(|input| input.path().starts_with(migration.source_root()))
        .map(|input| input.path().clone())
        .collect();
    if current != expected {
        return Err(CordialError::stale_source_snapshot(migration.source_root()));
    }
    Ok(())
}
