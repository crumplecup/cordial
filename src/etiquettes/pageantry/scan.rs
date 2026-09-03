//! syn-based scan for traits defined after the leading block.

use std::path::Path;

use syn::File;

use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures, quality_scan_trees};

use super::types::{PageantryRuleId, PageantrySiteRecord};

use tracing::instrument;

/// Scan one crate for misplaced trait definitions.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_pageantry(crate_root: &Path) -> CordialResult<Vec<PageantrySiteRecord>> {
    let mut findings = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        findings.extend(scan_source_tree(&tree_root, crate_root)?);
    }

    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.snippet().cmp(b.snippet()))
    });

    Ok(findings)
}

#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<PageantrySiteRecord>> {
    let mut findings = Vec::new();
    if !tree_root.is_dir() {
        return Ok(findings);
    }

    for entry in walkdir::WalkDir::new(tree_root)
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
        let source = std::fs::read_to_string(path)?;
        findings.extend(scan_rust_source(&source, path, tree_root, crate_root)?);
    }

    Ok(findings)
}

/// Scan one Rust source file and return records.
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<PageantrySiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(tree_root, file);
    scan_syntax(&syntax, file, crate_root, &module_prefix)
}

#[instrument(
    level = "debug",
    skip(syntax, file, crate_root, module_prefix),
    err(level = "warn")
)]
fn scan_syntax(
    syntax: &File,
    file: &Path,
    crate_root: &Path,
    module_prefix: &[String],
) -> CordialResult<Vec<PageantrySiteRecord>> {
    let mut findings = Vec::new();
    walk_items(
        &syntax.items,
        file,
        crate_root,
        module_prefix,
        &mut findings,
    )?;
    Ok(findings)
}

#[instrument(
    level = "debug",
    skip(items, file, crate_root, module_prefix, findings),
    err(level = "warn")
)]
fn walk_items(
    items: &[syn::Item],
    file: &Path,
    crate_root: &Path,
    module_prefix: &[String],
    findings: &mut Vec<PageantrySiteRecord>,
) -> CordialResult<()> {
    let mut body_started = false;
    for item in items {
        if is_cfg_test(item_attrs(item)) {
            continue;
        }
        match classify(item) {
            ItemClass::Header => {
                if let syn::Item::Mod(item_mod) = item
                    && let Some((_, nested)) = &item_mod.content
                {
                    let mut nested_prefix = module_prefix.to_vec();
                    nested_prefix.push(item_mod.ident.to_string());
                    walk_items(nested, file, crate_root, &nested_prefix, findings)?;
                }
            }
            ItemClass::Trait { name, line } => {
                if body_started {
                    let mut path = file.to_path_buf();
                    if let Ok(rel) = path.strip_prefix(crate_root) {
                        path = rel.to_path_buf();
                    }
                    findings.push(
                        PageantrySiteRecord::builder()
                            .rule_id(PageantryRuleId::Trait001)
                            .context(site_context(module_prefix))
                            .file(path)
                            .line(line)
                            .snippet(format!("trait {name}"))
                            .build()?,
                    );
                }
            }
            ItemClass::Body => {
                body_started = true;
            }
            ItemClass::Skip => {}
        }
    }
    Ok(())
}

#[derive(Debug)]
enum ItemClass {
    Header,
    Trait { name: String, line: u32 },
    Body,
    Skip,
}

#[instrument(level = "trace", skip(item), ret)]
fn classify(item: &syn::Item) -> ItemClass {
    match item {
        syn::Item::Use(_) | syn::Item::ExternCrate(_) | syn::Item::Mod(_) => ItemClass::Header,
        syn::Item::Trait(item) => ItemClass::Trait {
            name: item.ident.to_string(),
            line: item.ident.span().start().line as u32,
        },
        syn::Item::TraitAlias(item) => ItemClass::Trait {
            name: item.ident.to_string(),
            line: item.ident.span().start().line as u32,
        },
        syn::Item::Verbatim(_) => ItemClass::Skip,
        _ => ItemClass::Body,
    }
}

#[instrument(level = "trace", skip(prefix), ret)]
fn site_context(prefix: &[String]) -> String {
    if prefix.is_empty() {
        "<crate>".to_string()
    } else {
        prefix.join("::")
    }
}

#[instrument(level = "trace", skip(item))]
fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::ExternCrate(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::ForeignMod(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::TraitAlias(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        syn::Item::Verbatim(_) => &[],
        _ => &[],
    }
}

/// Whether `attrs` carries a bare `#[cfg(test)]`.
#[instrument(level = "trace", skip(attrs), ret)]
fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        list.path.is_ident("cfg") && list.tokens.to_string().replace(' ', "") == "test"
    })
}
