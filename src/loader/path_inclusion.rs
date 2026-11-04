//! Collect shared workspace splice evidence before policy consumers run.

use super::SourceLoadView;
use crate::ir::{
    PathInclusionFacts, PathInclusionMember, PathInclusionSource, canonical_or, normalize,
};
use crate::{CordialError, CordialResult, FileSnapshot};
use std::path::Path;

/// Collect workspace-wide module splices and declared Cargo dependencies.
/// Source and metadata failures remain errors, rather than an empty graph.
#[tracing::instrument(level = "debug", err(level = "warn"))]
pub fn workspace_path_inclusions(workspace_root: &Path) -> CordialResult<PathInclusionFacts> {
    workspace_path_inclusions_with_source(workspace_root, &SourceLoadView::for_standalone_scan())
}

#[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
#[hotpath::measure]
pub(crate) fn workspace_path_inclusions_with_source(
    workspace_root: &Path,
    source: &SourceLoadView,
) -> CordialResult<PathInclusionFacts> {
    let members = if workspace_root.join("Cargo.toml").is_file() {
        cargo_members(workspace_root)?
    } else {
        // Manifest-free source projects are explicit synthetic targets, just
        // as in target discovery. A broken existing manifest is still an error.
        vec![PathInclusionMember::new(
            crate::store::project_slug_from_path(workspace_root),
            canonical_or(workspace_root),
            Default::default(),
        )]
    };
    let mut sources = Vec::new();
    for member in &members {
        for file in
            source
                .parsed_rust_files_under(&member.root().join("src"), "path-inclusion", |_| true)?
        {
            sources.push(PathInclusionSource::new(
                member.name().clone(),
                FileSnapshot::new(canonical_or(file.path()), file.source_digest()),
                compatibility(&file),
            ));
        }
    }
    Ok(PathInclusionFacts::from_evidence(members, sources))
}

#[tracing::instrument(level = "debug", err(level = "warn"))]
fn cargo_members(workspace_root: &Path) -> CordialResult<Vec<PathInclusionMember>> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(workspace_root)
        .exec()
        .map_err(|error| CordialError::cargo_metadata(error))?;
    metadata
        .workspace_members
        .iter()
        .map(|id| {
            let package = metadata
                .packages
                .iter()
                .find(|package| &package.id == id)
                .ok_or_else(|| CordialError::missing_package_metadata(id.to_string()))?;
            let root = package.manifest_path.parent().ok_or_else(|| {
                CordialError::manifest_path_has_no_parent(package.manifest_path.to_string())
            })?;
            Ok(PathInclusionMember::new(
                package.name.to_string(),
                canonical_or(Path::new(root.as_str())),
                package
                    .dependencies
                    .iter()
                    .map(|dep| normalize(&dep.name))
                    .collect(),
            ))
        })
        .collect()
}

#[tracing::instrument(level = "trace", skip(file))]
fn compatibility(file: &super::SharedParsedFile) -> Vec<String> {
    super::source_walk(file.path(), "path-inclusion", || {
        targets(&file.syntax().items)
    })
}

#[tracing::instrument(level = "trace", skip(items))]
fn targets(items: &[syn::Item]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        if let syn::Item::Mod(module) = item {
            if let Some(target) = module.attrs.iter().find_map(path_value) {
                out.push(target);
            }
            if let Some((_, nested)) = &module.content {
                out.extend(targets(nested));
            }
        }
    }
    out
}

#[tracing::instrument(level = "trace", skip(attr))]
fn path_value(attr: &syn::Attribute) -> Option<String> {
    if !attr.path().is_ident("path") {
        return None;
    }
    match &attr.meta {
        syn::Meta::NameValue(value) => match &value.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(value),
                ..
            }) => Some(value.value()),
            _ => None,
        },
        _ => None,
    }
}
