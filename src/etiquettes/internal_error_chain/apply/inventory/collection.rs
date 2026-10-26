//! Identify conversion candidates and visit each shared file once for sites.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use syn::spanned::Spanned;
use syn::{Item, Type, Visibility};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::loader::SharedParsedFile;

use super::batch::BatchInventoryVisitor;
use super::resolution::CrateTypeIndex;
use super::{
    ConversionCandidate, ErrorConversionSite, ErrorConversionSiteKind, ErrorConversionUnit,
    FieldEvidence, InitializerEvidence, LifetimeEvidence, StringStorage,
};
use crate::etiquettes::internal_error_chain::apply::lifetime;
use crate::etiquettes::internal_error_chain::apply::{
    initializers::{FieldKind, InitializerBatchInput, collect_initializer_edits},
    is_error_string_field, is_string, known_error_impls, known_static_strings,
    single_struct_lifetime, span_range,
};
use crate::etiquettes::internal_error_chain::type_graph::item_derives_error;

/// Return one conversion unit and declaration line per candidate field.
#[instrument(level = "debug", skip(files, declarations))]
pub(super) fn collect_conversion_units(
    files: &[SharedParsedFile],
    declarations: &CrateTypeIndex,
) -> Vec<(ErrorConversionUnit, u32)> {
    let candidates = find_candidates(files, declarations);
    let mut all_sites = vec![Vec::new(); candidates.len()];
    for file in files {
        let identifiers: HashSet<&str> = file
            .source()
            .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
            .filter(|part| !part.is_empty())
            .collect();
        let relevant: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                (file.path() == candidate.declaration
                    || identifiers.contains(candidate.type_name.as_str()))
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
    for (unit, _) in &mut units {
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
            unit.lifetime.edits = lifetime::plan_for_unit(file, index, unit, unit.lifetime.target);
        }
    }
    units
}

#[instrument(level = "debug", skip(files, index))]
fn find_candidates(files: &[SharedParsedFile], index: &CrateTypeIndex) -> Vec<ConversionCandidate> {
    let mut candidates = Vec::new();
    for file in files {
        let start = candidates.len();
        let syntax = file.syntax();
        let error_impls = known_error_impls(syntax);
        let mut struct_counts = HashMap::new();
        for item in &syntax.items {
            if let Item::Struct(item) = item {
                *struct_counts
                    .entry(item.ident.to_string())
                    .or_insert(0usize) += 1;
            }
        }
        for item in &syntax.items {
            let Item::Struct(item) = item else { continue };
            let type_name = item.ident.to_string();
            if !error_impls.contains(&type_name) && !item_derives_error(&item.attrs) {
                continue;
            }
            let syn::Fields::Named(fields) = &item.fields else {
                continue;
            };
            for field in &fields.named {
                let Some(field_name) = field.ident.as_ref().map(ToString::to_string) else {
                    continue;
                };
                if matches!(field_name.as_str(), "source" | "kind")
                    || !is_error_string_field(&field.ty)
                {
                    continue;
                }
                let strategy = if is_string(&field.ty) {
                    ErrorFieldStrategy::Borrowed
                } else {
                    ErrorFieldStrategy::Owned
                };
                let kind = if field_name == "file" {
                    FieldKind::CallSiteFile
                } else {
                    FieldKind::StaticString
                };
                let lifetime = single_struct_lifetime(item);
                let storage = if is_string(&field.ty) {
                    StringStorage::Owned
                } else if let Type::Reference(reference) = &field.ty {
                    StringStorage::Borrowed {
                        lifetime: reference
                            .lifetime
                            .as_ref()
                            .map(|value| value.ident.to_string()),
                        mutable: reference.mutability.is_some(),
                    }
                } else {
                    continue;
                };
                candidates.push(ConversionCandidate {
                    declaration: file.path().to_path_buf(),
                    source_digest: file.source_digest(),
                    type_name: type_name.clone(),
                    field_name,
                    line: field.span().start().line as u32,
                    name_ambiguous: index.name_blocks(file, &type_name),
                    field: FieldEvidence {
                        kind,
                        storage,
                        struct_lifetime: lifetime.clone(),
                        private: matches!(item.vis, Visibility::Inherited)
                            && matches!(field.vis, Visibility::Inherited),
                        unique_struct: struct_counts.get(&type_name) == Some(&1),
                        type_range: span_range(file.source(), field.ty.span()),
                        lifetime_candidate: lifetime::is_candidate(
                            item,
                            field,
                            kind,
                            lifetime.as_deref(),
                            strategy,
                        ),
                    },
                    initializer: InitializerEvidence {
                        target: strategy,
                        edits: None,
                    },
                    initializer_kind: kind,
                    initializer_lifetime: lifetime,
                });
            }
        }
        if start == candidates.len() {
            continue;
        }
        let static_strings = known_static_strings(syntax);
        let edits = {
            let inputs: Vec<_> = candidates[start..]
                .iter()
                .map(|candidate| {
                    InitializerBatchInput::new(
                        &candidate.type_name,
                        &candidate.field_name,
                        candidate.initializer_kind,
                        candidate.initializer.target,
                        candidate.initializer_lifetime.as_deref(),
                    )
                })
                .collect();
            collect_initializer_edits(file, &inputs, &static_strings)
        };
        for (candidate, initializer_edits) in candidates[start..].iter_mut().zip(edits) {
            candidate.initializer.edits = initializer_edits;
        }
    }
    candidates
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
