//! Identify conversion candidates and visit each shared file once for sites.

use std::collections::HashSet;

use syn::Item;
use syn::spanned::Spanned;
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::batch::BatchInventoryVisitor;
use super::{
    ConversionCandidate, ErrorConversionSite, ErrorConversionSiteKind, ErrorConversionUnit,
};
use crate::etiquettes::internal_error_chain::apply::{
    is_error_string_field, known_error_impls, names::audit_names,
};
use crate::etiquettes::internal_error_chain::type_graph::item_derives_error;

/// Return one conversion unit and declaration line per candidate field.
#[instrument(level = "debug", skip(files))]
pub(super) fn collect_conversion_units(
    files: &[SharedParsedFile],
) -> Vec<(ErrorConversionUnit, u32)> {
    let candidates = find_candidates(files);
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
    candidates
        .into_iter()
        .zip(all_sites)
        .map(|(candidate, sites)| {
            (
                ErrorConversionUnit {
                    declaration: candidate.declaration,
                    type_name: candidate.type_name,
                    field_name: candidate.field_name,
                    sites,
                },
                candidate.line,
            )
        })
        .collect()
}

#[instrument(level = "debug", skip(files))]
fn find_candidates(files: &[SharedParsedFile]) -> Vec<ConversionCandidate> {
    let mut candidates = Vec::new();
    for file in files {
        let syntax = file.syntax();
        let error_impls = known_error_impls(syntax);
        let name_audit = audit_names(syntax);
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
                candidates.push(ConversionCandidate {
                    declaration: file.path().to_path_buf(),
                    type_name: type_name.clone(),
                    field_name,
                    line: field.span().start().line as u32,
                    name_ambiguous: name_audit.blocks(&type_name),
                });
            }
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
        related_type: None,
        resolved: false,
    }
}
