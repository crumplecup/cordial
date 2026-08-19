//! Library `src/` walk and syn type labels for the error type graph.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use syn::{Attribute, ItemImpl, Type, TypePath};
use walkdir::WalkDir;

use crate::enricher::is_cfg_test;
use crate::error::CordialResult;
use crate::loader::path_has_fixtures;

use tracing::instrument;

/// Walk library `src/` files (types that implement `Error` may live next to
/// their call sites, not only in `src/error.rs`).
///
/// When `src/lib.rs` exists, only modules reachable from it are scanned, so
/// bin-only wrappers (`src/main.rs`, `src/boundary.rs` linked from main) stay
/// out of the architecture catalog. Crates with no `lib.rs`/`main.rs` fall
/// back to every `.rs` file under `src/`.
#[instrument(level = "debug", skip(visit), err(level = "warn"))]
pub(crate) fn for_each_src_rust_file(
    crate_root: &Path,
    mut visit: impl FnMut(&Path, &Path) -> CordialResult<()>,
) -> CordialResult<()> {
    let src_root = crate_root.join("src");
    if !src_root.is_dir() {
        return Ok(());
    }
    for path in collect_library_src_files(crate_root, &src_root)? {
        visit(&path, &src_root)?;
    }
    Ok(())
}

#[instrument(level = "debug", err(level = "warn"))]
fn collect_library_src_files(crate_root: &Path, src_root: &Path) -> CordialResult<Vec<PathBuf>> {
    let lib = src_root.join("lib.rs");
    let main = src_root.join("main.rs");
    let roots = if lib.is_file() {
        vec![lib]
    } else if main.is_file() {
        vec![main]
    } else {
        return collect_all_src_rs(crate_root, src_root);
    };
    let mut files = BTreeSet::new();
    for root in roots {
        collect_mod_tree(&root, crate_root, &mut files)?;
    }
    Ok(files.into_iter().collect())
}

#[instrument(level = "debug", err(level = "warn"))]
fn collect_all_src_rs(crate_root: &Path, src_root: &Path) -> CordialResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(src_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        if path_has_fixtures(path, crate_root) {
            continue;
        }
        files.push(path.to_path_buf());
    }
    files.sort();
    Ok(files)
}

#[instrument(level = "debug", skip(file, files), err(level = "warn"))]
pub(crate) fn collect_mod_tree(
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

/// Whether a trait path is `Error` / `std::error::Error` / `core::error::Error`.
#[instrument(level = "debug", skip(path))]
pub(crate) fn trait_is_std_error(path: &syn::Path) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == "Error")
}

/// `#[derive(Error)]` / `#[derive(thiserror::Error)]`.
#[instrument(level = "debug", skip(attrs))]
pub(crate) fn item_derives_error(attrs: &[Attribute]) -> bool {
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

/// Last path segment of a type label (`io::IoSource` → `IoSource`).
#[instrument(level = "debug")]
pub(crate) fn last_ident(label: &str) -> &str {
    label.rsplit("::").next().unwrap_or(label)
}

/// Keep type-graph nodes that belong to a type (or variant of a type) that
/// implements `Error`.
#[instrument(level = "debug", skip(error_impls))]
pub(crate) fn type_path_is_error_related(type_path: &str, error_impls: &BTreeSet<String>) -> bool {
    type_path
        .split("::")
        .any(|segment| error_impls.contains(segment))
}
#[instrument(level = "debug", skip(item_impl))]
pub(super) fn extract_source_return_type(item_impl: &ItemImpl) -> Option<String> {
    for item in &item_impl.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };
        if method.sig.ident != "source" {
            continue;
        }
        let syn::Stmt::Expr(syn::Expr::Match(match_expr), _) = method.block.stmts.first()? else {
            continue;
        };
        for arm in &match_expr.arms {
            if let syn::Expr::Path(path) = &*arm.body {
                return Some(type_path_label(&path.path));
            }
        }
    }
    None
}

#[instrument(level = "debug")]
pub(super) fn qualified_type_name(module_prefix: &[String], name: &str) -> String {
    if module_prefix.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", module_prefix.join("::"), name)
    }
}

#[instrument(level = "trace", ret)]
pub(crate) fn is_foreign_type_label(label: &str) -> bool {
    [
        "std::",
        "serde_json::",
        "serde_yaml::",
        "syn::",
        "csv::",
        "cargo_metadata::",
        "reqwest::",
        "url::",
        "toml::",
    ]
    .iter()
    .any(|prefix| label.starts_with(prefix))
        || (label.ends_with("Error") && label.contains("::"))
}

#[instrument(level = "trace", skip(ty))]
pub(crate) fn is_string_type(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => path.is_ident("String"),
        Type::Reference(reference) => is_string_type(&reference.elem),
        Type::Paren(paren) => is_string_type(&paren.elem),
        Type::Group(group) => is_string_type(&group.elem),
        _ => false,
    }
}

#[instrument(level = "debug", skip(ty))]
pub(crate) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => type_path_label(&type_path.path),
        Type::Reference(reference) => type_label(&reference.elem),
        Type::Paren(paren) => type_label(&paren.elem),
        Type::Group(group) => type_label(&group.elem),
        _ => "?".to_string(),
    }
}

#[instrument(level = "debug", skip(path))]
pub(super) fn type_path_label(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}
