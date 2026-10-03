//! syn-based scan for trait placement and barrel-file declarations.

use std::path::Path;

use syn::File;
use syn::spanned::Spanned;

use crate::config::PageantryThresholds;
use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures, quality_scan_trees};

use super::shim::{proc_macro_entry_attr, shim_overflow};
use super::types::{PageantryRuleId, PageantrySiteRecord};

use tracing::instrument;

/// Scan one crate for misplaced traits and logic in barrel files.
#[instrument(level = "debug", skip(thresholds), err(level = "warn"))]
pub fn scan_crate_pageantry(
    crate_root: &Path,
    thresholds: &PageantryThresholds,
) -> CordialResult<Vec<PageantrySiteRecord>> {
    let mut findings = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        findings.extend(scan_source_tree(&tree_root, crate_root, thresholds)?);
    }

    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.snippet().cmp(b.snippet()))
    });

    Ok(findings)
}

#[instrument(level = "debug", skip(thresholds), err(level = "warn"))]
pub fn scan_source_tree(
    tree_root: &Path,
    crate_root: &Path,
    thresholds: &PageantryThresholds,
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
        findings.extend(scan_rust_source(
            &source, path, tree_root, crate_root, thresholds,
        )?);
    }

    Ok(findings)
}

/// Scan one Rust source file and return records.
#[instrument(level = "debug", skip(source, file, thresholds), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
    thresholds: &PageantryThresholds,
) -> CordialResult<Vec<PageantrySiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(tree_root, file);
    scan_syntax(&syntax, file, crate_root, &module_prefix, thresholds)
}

#[instrument(
    level = "debug",
    skip(syntax, file, crate_root, module_prefix, thresholds),
    err(level = "warn")
)]
fn scan_syntax(
    syntax: &File,
    file: &Path,
    crate_root: &Path,
    module_prefix: &[String],
    thresholds: &PageantryThresholds,
) -> CordialResult<Vec<PageantrySiteRecord>> {
    let mut findings = Vec::new();
    walk_items(
        &syntax.items,
        file,
        crate_root,
        module_prefix,
        is_barrel_file(file),
        thresholds,
        &mut findings,
    )?;
    Ok(findings)
}

#[instrument(
    level = "debug",
    skip(items, file, crate_root, module_prefix, thresholds, findings),
    err(level = "warn")
)]
fn walk_items(
    items: &[syn::Item],
    file: &Path,
    crate_root: &Path,
    module_prefix: &[String],
    barrel: bool,
    thresholds: &PageantryThresholds,
    findings: &mut Vec<PageantrySiteRecord>,
) -> CordialResult<()> {
    let mut body_started = false;
    for item in items {
        if is_cfg_test(item_attrs(item)) {
            continue;
        }
        if barrel
            && let Some((rule_id, line, snippet)) = barrel_hit(item, thresholds)
            && thresholds.rule_enabled(rule_id.as_str())
        {
            findings.push(site_record(
                rule_id,
                module_prefix,
                file,
                crate_root,
                line,
                snippet,
            )?);
        }
        match classify(item) {
            ItemClass::Header => {
                if let syn::Item::Mod(item_mod) = item
                    && let Some((_, nested)) = &item_mod.content
                {
                    let mut nested_prefix = module_prefix.to_vec();
                    nested_prefix.push(item_mod.ident.to_string());
                    walk_items(
                        nested,
                        file,
                        crate_root,
                        &nested_prefix,
                        barrel,
                        thresholds,
                        findings,
                    )?;
                }
            }
            ItemClass::Trait { name, line } => {
                if body_started && thresholds.rule_enabled(PageantryRuleId::Trait001.as_str()) {
                    findings.push(site_record(
                        PageantryRuleId::Trait001,
                        module_prefix,
                        file,
                        crate_root,
                        line,
                        format!("trait {name}"),
                    )?);
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

#[instrument(level = "trace", skip(file), ret)]
fn is_barrel_file(file: &Path) -> bool {
    matches!(
        file.file_name().and_then(|name| name.to_str()),
        Some("lib.rs" | "mod.rs")
    )
}

/// The barrel rule that `item` breaks, if any.
///
/// A proc-macro entry point is exempt from `PAGEANTRY-BARREL-001`
/// because rustc pins it to the crate root, but it must stay a shim:
/// a body longer than `max_shim_lines` raises `PAGEANTRY-BARREL-SHIM-001`.
#[instrument(level = "trace", skip(item, thresholds))]
fn barrel_hit(
    item: &syn::Item,
    thresholds: &PageantryThresholds,
) -> Option<(PageantryRuleId, u32, String)> {
    if let syn::Item::Fn(item_fn) = item
        && proc_macro_entry_attr(&item_fn.attrs).is_some()
    {
        return shim_overflow(item_fn, thresholds.max_shim_lines());
    }
    barrel_declaration(item).map(|(line, snippet)| (PageantryRuleId::Barrel001, line, snippet))
}

#[instrument(level = "trace", skip(item))]
fn barrel_declaration(item: &syn::Item) -> Option<(u32, String)> {
    match item {
        syn::Item::Use(_) | syn::Item::ExternCrate(_) | syn::Item::Mod(_) => None,
        syn::Item::Verbatim(_) => None,
        syn::Item::Fn(item) => Some((
            item.sig.ident.span().start().line as u32,
            format!("fn {}", item.sig.ident),
        )),
        syn::Item::Struct(item) => Some((
            item.ident.span().start().line as u32,
            format!("struct {}", item.ident),
        )),
        syn::Item::Enum(item) => Some((
            item.ident.span().start().line as u32,
            format!("enum {}", item.ident),
        )),
        syn::Item::Union(item) => Some((
            item.ident.span().start().line as u32,
            format!("union {}", item.ident),
        )),
        syn::Item::Trait(item) => Some((
            item.ident.span().start().line as u32,
            format!("trait {}", item.ident),
        )),
        syn::Item::TraitAlias(item) => Some((
            item.ident.span().start().line as u32,
            format!("trait {}", item.ident),
        )),
        syn::Item::Type(item) => Some((
            item.ident.span().start().line as u32,
            format!("type {}", item.ident),
        )),
        syn::Item::Const(item) => Some((
            item.ident.span().start().line as u32,
            format!("const {}", item.ident),
        )),
        syn::Item::Static(item) => Some((
            item.ident.span().start().line as u32,
            format!("static {}", item.ident),
        )),
        syn::Item::Impl(item) => {
            let line = item.impl_token.span.start().line as u32;
            let snippet = match item.self_ty.as_ref() {
                syn::Type::Path(ty) => ty
                    .path
                    .segments
                    .last()
                    .map(|segment| format!("impl {}", segment.ident))
                    .unwrap_or_else(|| "impl".to_string()),
                _ => "impl".to_string(),
            };
            Some((line, snippet))
        }
        syn::Item::Macro(item) => {
            let name = item
                .ident
                .as_ref()
                .map(|ident| ident.to_string())
                .or_else(|| {
                    item.mac
                        .path
                        .segments
                        .last()
                        .map(|segment| segment.ident.to_string())
                })
                .unwrap_or_else(|| "macro".to_string());
            let line = item
                .ident
                .as_ref()
                .map(|ident| ident.span().start().line as u32)
                .unwrap_or_else(|| item.mac.path.span().start().line as u32);
            Some((line, format!("macro {name}")))
        }
        syn::Item::ForeignMod(item) => Some((
            item.abi.extern_token.span.start().line as u32,
            "extern block".to_string(),
        )),
        _ => Some((1, "item".to_string())),
    }
}

#[instrument(level = "debug", skip(rule_id, file))]
fn site_record(
    rule_id: PageantryRuleId,
    module_prefix: &[String],
    file: &Path,
    crate_root: &Path,
    line: u32,
    snippet: String,
) -> CordialResult<PageantrySiteRecord> {
    let mut path = file.to_path_buf();
    if let Ok(rel) = path.strip_prefix(crate_root) {
        path = rel.to_path_buf();
    }
    PageantrySiteRecord::builder()
        .rule_id(rule_id)
        .context(site_context(module_prefix))
        .file(path)
        .line(line)
        .snippet(snippet)
        .build()
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
