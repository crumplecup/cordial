//! Adapters and policy selection over loader-owned modularity evidence.
use super::types::ModularitySiteRecord;
use crate::{CordialResult, ModularityKind, ModularityThresholds};
use std::path::Path;

/// Scan a standalone source using the loader's compatibility collector.
#[tracing::instrument(level = "debug", skip(source, thresholds, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
    thresholds: &ModularityThresholds,
) -> CordialResult<Vec<ModularitySiteRecord>> {
    let records = crate::ir::scan_modularity_source(source, file, src_root, crate_root)?;
    select_records(&records, crate_root, thresholds)
}

/// Apply current thresholds without revisiting syntax or source text.
#[tracing::instrument(level = "debug", skip(records, thresholds), err(level = "warn"))]
pub(crate) fn select_records(
    records: &[ModularitySiteRecord],
    root: &Path,
    thresholds: &ModularityThresholds,
) -> CordialResult<Vec<ModularitySiteRecord>> {
    let files: std::collections::BTreeMap<_, _> = records
        .iter()
        .filter(|record| record.kind() == ModularityKind::File)
        .map(|record| (record.file(), record.lines()))
        .collect();
    let mut selected = Vec::new();
    for record in records {
        let generated = thresholds.is_generated_file(&root.join(record.file()), root);
        let include = match record.kind() {
            ModularityKind::File => {
                !generated && record.lines() >= thresholds.file_inventory_min_lines()
            }
            ModularityKind::ModuleSize => !generated,
            ModularityKind::TypesPerFile => record.lines() > thresholds.max_types_per_file(),
            ModularityKind::Function => {
                let file_lines = files.get(record.file()).ok_or_else(|| {
                    crate::CordialError::not_found(
                        format!("modularity file evidence for {}", record.file().display()).into(),
                    )
                })?;
                record.lines() >= thresholds.function_scan_min_lines(*file_lines)
            }
            _ => false,
        };
        if include {
            selected.push(record.clone());
        }
    }
    Ok(selected)
}
