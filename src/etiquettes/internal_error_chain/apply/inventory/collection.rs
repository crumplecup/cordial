//! Identify conversion candidates and visit each shared file once for sites.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::error::{CordialError, CordialResult};
use crate::loader::SharedParsedFile;

use super::batch::BatchInventoryVisitor;
use super::resolution::CrateTypeIndex;
use super::{
    ConversionCandidate, ErrorConversionSite, ErrorConversionSiteKind, ErrorConversionUnit,
    FieldEvidence, InitializerEvidence, LifetimeEvidence, StringStorage,
};
use crate::etiquettes::internal_error_chain::apply::graph_apply::{DeclarationKind, GraphFields};
use crate::etiquettes::internal_error_chain::apply::initializers::{
    FieldKind, InitializerBatchInput, collect_initializer_edits,
};
use crate::etiquettes::internal_error_chain::apply::lifetime;

/// Return one conversion unit and declaration line per candidate field.
#[instrument(level = "debug", skip(files, declarations))]
pub(super) fn collect_conversion_units(
    files: &[SharedParsedFile],
    declarations: &CrateTypeIndex,
) -> CordialResult<Vec<(ErrorConversionUnit, u32)>> {
    let candidates = find_candidates(files, declarations)?;
    let mut all_sites = vec![Vec::new(); candidates.len()];
    for file in files {
        let relevant: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                (file.path() == candidate.declaration
                    || declarations.source_mentions(file, &candidate.type_name))
                .then_some(index)
            })
            .collect();
        if relevant.is_empty() {
            continue;
        }
        let mut sites = BatchInventoryVisitor::collect(file, &candidates, &relevant);
        for index in relevant {
            let candidate = &candidates[index];
            let external = file.path() != candidate.declaration;
            if !external
                && candidate.name_ambiguous
                && !sites[index]
                    .iter()
                    .any(|site| site.kind == ErrorConversionSiteKind::Alias && !site.resolved)
            {
                sites[index].push(unresolved_site(file, ErrorConversionSiteKind::Alias));
            }
            if external && sites[index].is_empty() {
                sites[index].push(unresolved_site(
                    file,
                    ErrorConversionSiteKind::CrossFileReference,
                ));
            }
            all_sites[index].append(&mut sites[index]);
        }
    }
    let lifetime_initializers: Vec<_> = candidates
        .iter()
        .map(|candidate| candidate.lifetime_initializer_edits.clone())
        .collect();
    let mut units: Vec<_> = candidates
        .into_iter()
        .zip(all_sites)
        .map(|(candidate, sites)| {
            (
                ErrorConversionUnit {
                    declaration: candidate.declaration,
                    source_digest: candidate.source_digest,
                    type_name: candidate.type_name,
                    field_name: candidate.field_name,
                    name_ambiguous: candidate.name_ambiguous,
                    field: candidate.field,
                    lifetime: LifetimeEvidence {
                        target: candidate.initializer.target,
                        edits: None,
                    },
                    initializer: candidate.initializer,
                    sites,
                },
                candidate.line,
            )
        })
        .collect();
    let files_by_path: HashMap<&Path, &SharedParsedFile> =
        files.iter().map(|file| (file.path(), file)).collect();
    for (unit, _) in &mut units {
        for site in &mut unit.sites {
            let Some(path) = site.reference_path.as_ref() else {
                continue;
            };
            if let Some(file) = files_by_path.get(site.path.as_path())
                && let Some(target) = declarations.resolve(file, path)
                && target.declaration() == &unit.declaration
            {
                site.target = Some(target);
            }
        }
        for site in &mut unit.sites {
            if site.kind != ErrorConversionSiteKind::ErrorEdge || site.path != unit.declaration {
                continue;
            }
            let Some(edge) = &mut site.edge else {
                continue;
            };
            if let Some(file) = files_by_path.get(unit.declaration.as_path())
                && let Some(target) = declarations.resolve(file, edge.type_path())
            {
                edge.set_target(target);
            }
            // Matching spelling is a graph candidate, not type resolution.
            // Imports, macros, and cfg can still affect the true target.
            site.resolved = false;
        }
    }
    let lifetime_paths: HashSet<&Path> = units
        .iter()
        .filter(|(unit, _)| {
            unit.lifetime_candidate(unit.lifetime.target)
                && !unit.name_ambiguous
                && unit.field.private
                && unit.sites.iter().all(|site| site.path == unit.declaration)
        })
        .map(|(unit, _)| unit.declaration.as_path())
        .collect();
    let lifetime_indexes: HashMap<_, _> = files
        .iter()
        .filter(|file| lifetime_paths.contains(file.path()))
        .map(|file| (file.path(), lifetime::LifetimeFileIndex::new(file.syntax())))
        .collect();
    for ((unit, _), initializer_edits) in units.iter_mut().zip(lifetime_initializers) {
        if !unit.lifetime_candidate(unit.lifetime.target)
            || unit.name_ambiguous
            || !unit.field.private
            || unit.sites.iter().any(|site| site.path != unit.declaration)
        {
            continue;
        }
        if let (Some(file), Some(index)) = (
            files_by_path.get(unit.declaration.as_path()),
            lifetime_indexes.get(unit.declaration.as_path()),
        ) {
            unit.lifetime.edits =
                lifetime::plan_for_unit(file, index, unit, unit.lifetime.target, initializer_edits);
        }
    }
    Ok(units)
}

#[instrument(level = "debug", skip(files, index), err(level = "warn"))]
fn find_candidates(
    files: &[SharedParsedFile],
    index: &CrateTypeIndex,
) -> CordialResult<Vec<ConversionCandidate>> {
    let mut candidates = Vec::new();
    for file in files {
        let start = candidates.len();
        let declarations = index
            .graph_declarations(file)
            .ok_or_else(|| CordialError::stale_source_snapshot(file.path()))?;
        let mut struct_counts = HashMap::new();
        for item in declarations
            .all()
            .iter()
            .filter(|item| item.kind() == DeclarationKind::Struct)
        {
            *struct_counts.entry(item.name().clone()).or_insert(0usize) += 1;
        }
        for item in declarations
            .all()
            .iter()
            .filter(|item| item.kind() == DeclarationKind::Struct)
        {
            let type_name = item.name().clone();
            if !item.error_impl() && !item.derives_error() {
                continue;
            }
            let GraphFields::Named(fields) = item.fields() else {
                continue;
            };
            for field in fields {
                let Some(field_name) = field.name().map(str::to_string) else {
                    continue;
                };
                if matches!(field_name.as_str(), "source" | "kind") {
                    continue;
                }
                let Some(storage) = field.candidate_storage() else {
                    continue;
                };
                let strategy = if matches!(storage, StringStorage::Owned) {
                    ErrorFieldStrategy::Borrowed
                } else {
                    ErrorFieldStrategy::Owned
                };
                let kind = if field_name == "file" {
                    FieldKind::CallSiteFile
                } else {
                    FieldKind::StaticString
                };
                let lifetime = item.single_struct_lifetime().map(str::to_string);
                let lifetime_candidate = kind == FieldKind::StaticString
                    && lifetime.is_none()
                    && match (strategy, storage) {
                        (ErrorFieldStrategy::Borrowed, StringStorage::Owned) => {
                            item.generics_empty()
                        }
                        (
                            ErrorFieldStrategy::Owned,
                            StringStorage::Borrowed {
                                lifetime: Some(named),
                                ..
                            },
                        ) => named != "static",
                        _ => false,
                    };
                candidates.push(ConversionCandidate {
                    declaration: file.path().to_path_buf(),
                    source_digest: file.source_digest(),
                    type_name: type_name.clone(),
                    field_name,
                    line: field.line() as u32,
                    name_ambiguous: index.name_blocks(file, &type_name),
                    field: FieldEvidence {
                        kind,
                        storage: storage.clone(),
                        struct_lifetime: lifetime.clone(),
                        private: item.is_inherited() && field.is_inherited(),
                        unique_struct: struct_counts.get(&type_name) == Some(&1),
                        type_range: field.type_range(),
                        lifetime_candidate,
                    },
                    initializer: InitializerEvidence {
                        target: strategy,
                        edits: None,
                    },
                    initializer_kind: kind,
                    initializer_lifetime: lifetime,
                    lifetime_initializer_edits: None,
                });
            }
        }
        if start == candidates.len() {
            continue;
        }
        let static_strings = index
            .static_strings(file)
            .ok_or_else(|| CordialError::stale_source_snapshot(file.path()))?;
        let mut inputs: Vec<_> = candidates[start..]
            .iter()
            .map(|candidate| {
                InitializerBatchInput::new(
                    &candidate.type_name,
                    &candidate.field_name,
                    candidate.initializer_kind,
                    candidate.initializer.target,
                    candidate.initializer_lifetime.as_deref(),
                    false,
                )
            })
            .collect();
        let ordinary_count = inputs.len();
        let mut lifetime_inputs = Vec::new();
        for (offset, candidate) in candidates[start..].iter().enumerate() {
            let lifetime = match candidate.initializer.target {
                ErrorFieldStrategy::Borrowed if candidate.field.lifetime_candidate => {
                    let names = BTreeSet::from([candidate.type_name.clone()]);
                    let anchors = index
                        .lifetime_facts(file)
                        .map(|facts| facts.anchors_for(&names))
                        .unwrap_or_default();
                    (anchors.len() == 1)
                        .then(|| anchors.into_iter().next())
                        .flatten()
                }
                ErrorFieldStrategy::Owned if candidate.field.lifetime_candidate => {
                    match &candidate.field.storage {
                        StringStorage::Borrowed {
                            lifetime: Some(lifetime),
                            ..
                        } => Some(lifetime.clone()),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(lifetime) = lifetime {
                lifetime_inputs.push((offset, lifetime));
            }
        }
        for (offset, lifetime) in &lifetime_inputs {
            let candidate = &candidates[start + *offset];
            inputs.push(InitializerBatchInput::new(
                &candidate.type_name,
                &candidate.field_name,
                FieldKind::StaticString,
                candidate.initializer.target,
                Some(lifetime),
                candidate.initializer.target == ErrorFieldStrategy::Borrowed,
            ));
        }
        let edits = collect_initializer_edits(file, &inputs, static_strings);
        let (ordinary, lifetime_edits) = edits.split_at(ordinary_count);
        for (candidate, initializer_edits) in candidates[start..].iter_mut().zip(ordinary) {
            candidate.initializer.edits = initializer_edits.clone();
        }
        for ((offset, _), edits) in lifetime_inputs.into_iter().zip(lifetime_edits) {
            candidates[start + offset].lifetime_initializer_edits = edits.clone();
        }
    }
    Ok(candidates)
}

#[instrument(level = "debug", skip(file, kind))]
fn unresolved_site(file: &SharedParsedFile, kind: ErrorConversionSiteKind) -> ErrorConversionSite {
    ErrorConversionSite {
        path: file.path().to_path_buf(),
        line: 0,
        column: 0,
        kind,
        edge: None,
        from_conversion: None,
        source_method: None,
        try_conversion: None,
        reference_path: None,
        target: None,
        resolved: false,
    }
}
