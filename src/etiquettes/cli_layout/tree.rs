//! Module-tree walk and small syn helpers for the CLI-layout scan.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use syn::{Attribute, Type};

use crate::enricher::is_cfg_test;
use crate::error::CordialResult;
use crate::loader::path_has_fixtures;

use tracing::instrument;
#[instrument(level = "debug", skip(file, files), err(level = "warn"))]
pub(super) fn collect_mod_tree(
    file: &Path,
    crate_root: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> CordialResult<()> {
    if path_has_fixtures(file, crate_root) {
        return Ok(());
    }
    if !files.insert(file.to_path_buf()) {
        return Ok(());
    }
    let source = std::fs::read_to_string(file)?;
    let syntax = syn::parse_file(&source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    walk_mod_items(file, &syntax.items, &[], crate_root, files)
}

#[instrument(level = "debug", skip(items, files), err(level = "warn"))]
fn walk_mod_items(
    current_file: &Path,
    items: &[syn::Item],
    inline_path: &[String],
    crate_root: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> CordialResult<()> {
    for item in items {
        let syn::Item::Mod(item_mod) = item else {
            continue;
        };
        if is_cfg_test(&item_mod.attrs) {
            continue;
        }
        let name = item_mod.ident.to_string();
        match &item_mod.content {
            Some((_, nested)) => {
                let mut nested_path = inline_path.to_vec();
                nested_path.push(name);
                walk_mod_items(current_file, nested, &nested_path, crate_root, files)?;
            }
            None => {
                if let Some(child) =
                    resolve_mod_file(current_file, inline_path, &name, &item_mod.attrs)
                {
                    collect_mod_tree(&child, crate_root, files)?;
                }
            }
        }
    }
    Ok(())
}

#[instrument(level = "debug", skip(attrs))]
fn resolve_mod_file(
    current_file: &Path,
    inline_path: &[String],
    name: &str,
    attrs: &[Attribute],
) -> Option<PathBuf> {
    if let Some(path) = path_attr(attrs) {
        let parent = current_file.parent()?;
        let resolved = if path.is_absolute() {
            path
        } else {
            parent.join(path)
        };
        return resolved.is_file().then_some(resolved);
    }
    let stem = current_file.file_stem()?.to_str()?;
    let mut dir = current_file.parent()?.to_path_buf();
    if !matches!(stem, "lib" | "main" | "mod") {
        dir = current_file.with_extension("");
    }
    for part in inline_path {
        dir.push(part);
    }
    let rs = dir.join(format!("{name}.rs"));
    if rs.is_file() {
        return Some(rs);
    }
    let mod_rs = dir.join(name).join("mod.rs");
    mod_rs.is_file().then_some(mod_rs)
}

#[instrument(level = "debug", skip(attrs))]
fn path_attr(attrs: &[Attribute]) -> Option<PathBuf> {
    for attr in attrs {
        if !attr.path().is_ident("path") {
            continue;
        }
        let syn::Meta::NameValue(meta) = &attr.meta else {
            continue;
        };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(lit),
            ..
        }) = &meta.value
        else {
            continue;
        };
        return Some(PathBuf::from(lit.value()));
    }
    None
}

#[instrument(level = "debug", skip(path))]
pub(super) fn trait_is_std_error(path: &syn::Path) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == "Error")
}

#[instrument(level = "debug", skip(attrs))]
pub(super) fn item_derives_error(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        let mut found = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "Error")
            {
                found = true;
            }
            Ok(())
        });
        found
    })
}

#[instrument(level = "debug")]
pub(super) fn last_ident(label: &str) -> &str {
    label.rsplit("::").next().unwrap_or(label)
}

#[instrument(level = "debug", skip(ty))]
pub(super) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => type_path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}
