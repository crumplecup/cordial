//! Walk the crate module tree into [`ModuleNode`]s.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::{Attribute, Item, ItemMacro, ItemMod};

use crate::error::CordialResult;

use super::vis::{VisKind, is_cfg_test, is_doc_hidden, item_vis, leaf_name_count, vis_kind};

use tracing::instrument;
pub(super) struct ModuleNode {
    pub(super) path: String,
    pub(super) file: PathBuf,
    pub(super) line: u32,
    pub(super) declared_vis: VisKind,
    pub(super) parent_declared_pub: bool,
    pub(super) ancestors_all_pub: bool,
    pub(super) is_crate_root: bool,
    pub(super) leaf_pub: usize,
    pub(super) leaf_crate: usize,
    pub(super) children: Vec<ModuleNode>,
}

pub(super) struct ScanHeader<'a> {
    file: &'a Path,
    path: String,
    declared_vis: VisKind,
    parent_declared_pub: bool,
    ancestors_all_pub: bool,
    is_crate_root: bool,
    line: u32,
}

#[instrument(level = "debug", skip(file, path, declared_vis), err(level = "warn"))]
pub(super) fn scan_module_file(
    file: &Path,
    path: String,
    declared_vis: VisKind,
    parent_declared_pub: bool,
    ancestors_all_pub: bool,
    is_crate_root: bool,
) -> CordialResult<ModuleNode> {
    let source = std::fs::read_to_string(file)?;
    let syntax = syn::parse_file(&source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    scan_items(
        &syntax.items,
        ScanHeader {
            file,
            path,
            declared_vis,
            parent_declared_pub,
            ancestors_all_pub,
            is_crate_root,
            line: 1,
        },
    )
}

#[instrument(level = "debug", skip(items, header), err(level = "warn"))]
fn scan_items(items: &[Item], header: ScanHeader<'_>) -> CordialResult<ModuleNode> {
    let ScanHeader {
        file,
        path,
        declared_vis,
        parent_declared_pub,
        ancestors_all_pub,
        is_crate_root,
        line,
    } = header;
    let mut leaf_pub = 0usize;
    let mut leaf_crate = 0usize;
    let mut children = Vec::new();

    for item in items {
        match item {
            Item::Mod(item_mod) => {
                if is_cfg_test(&item_mod.attrs) || is_doc_hidden(&item_mod.attrs) {
                    continue;
                }
                let child_vis = vis_kind(&item_mod.vis);
                let child_path = if path == "crate" {
                    format!("crate::{}", item_mod.ident)
                } else {
                    format!("{path}::{}", item_mod.ident)
                };
                let child_line = item_mod.span().start().line as u32;
                let child_ancestors_pub = ancestors_all_pub && child_vis.is_unrestricted_pub();
                let child = scan_child_mod(
                    item_mod,
                    file,
                    child_path,
                    child_vis,
                    declared_vis.is_unrestricted_pub(),
                    child_ancestors_pub,
                    child_line,
                )?;
                children.push(child);
            }
            Item::Macro(item_macro) => {
                let (n_pub, n_crate) = verus_macro_leaf_names(item_macro);
                leaf_pub += n_pub;
                leaf_crate += n_crate;
            }
            _ => {
                let vis = item_vis(item);
                let n = leaf_name_count(item);
                if n == 0 {
                    continue;
                }
                match vis {
                    Some(VisKind::Pub) => {
                        leaf_pub += n;
                        leaf_crate += n;
                    }
                    Some(VisKind::PubCrate) => {
                        leaf_crate += n;
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(ModuleNode {
        path,
        file: file.to_path_buf(),
        line,
        declared_vis,
        parent_declared_pub,
        ancestors_all_pub,
        is_crate_root,
        leaf_pub,
        leaf_crate,
        children,
    })
}

#[instrument(
    level = "debug",
    skip(item_mod, path, declared_vis),
    err(level = "warn")
)]
fn scan_child_mod(
    item_mod: &ItemMod,
    parent_file: &Path,
    path: String,
    declared_vis: VisKind,
    parent_declared_pub: bool,
    ancestors_all_pub: bool,
    line: u32,
) -> CordialResult<ModuleNode> {
    if let Some((_, items)) = &item_mod.content {
        return scan_items(
            items,
            ScanHeader {
                file: parent_file,
                path,
                declared_vis,
                parent_declared_pub,
                ancestors_all_pub,
                is_crate_root: false,
                line,
            },
        );
    }
    let Some(child_file) = resolve_mod_file(parent_file, &item_mod.ident.to_string(), item_mod)
    else {
        return Ok(ModuleNode {
            path,
            file: parent_file.to_path_buf(),
            line,
            declared_vis,
            parent_declared_pub,
            ancestors_all_pub,
            is_crate_root: false,
            leaf_pub: 0,
            leaf_crate: 0,
            children: Vec::new(),
        });
    };
    scan_module_file(
        &child_file,
        path,
        declared_vis,
        parent_declared_pub,
        ancestors_all_pub,
        false,
    )
}

#[instrument(level = "debug", skip(item_mod))]
fn resolve_mod_file(parent_file: &Path, ident: &str, item_mod: &ItemMod) -> Option<PathBuf> {
    if let Some(path_val) = path_attr(&item_mod.attrs) {
        let resolved = parent_file.parent()?.join(path_val);
        return resolved.is_file().then_some(resolved);
    }
    let dir = parent_file.parent()?;
    let stem = parent_file.file_stem()?.to_str()?;
    let search_dir = if matches!(stem, "lib" | "main" | "mod") {
        dir.to_path_buf()
    } else {
        dir.join(stem)
    };
    let rs = search_dir.join(format!("{ident}.rs"));
    if rs.is_file() {
        return Some(rs);
    }
    let nested = search_dir.join(ident).join("mod.rs");
    nested.is_file().then_some(nested)
}

#[instrument(level = "debug", skip(attrs))]
fn path_attr(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("path") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
        {
            return Some(s.value());
        }
    }
    None
}

#[instrument(level = "debug", skip(node))]
pub(super) fn collect_files(node: &ModuleNode, files: &mut Vec<PathBuf>) {
    files.push(node.file.clone());
    for child in &node.children {
        collect_files(child, files);
    }
}

#[instrument(level = "debug", skip(node))]
pub(super) fn external_name_count(node: &ModuleNode) -> usize {
    let mut n = node.leaf_pub;
    for child in &node.children {
        if child.declared_vis.is_unrestricted_pub() {
            n += external_name_count(child);
        }
    }
    n
}

/// `(pub_names, pub_crate_names)` found inside `item_macro`, if it's a
/// `verus! { .. }` invocation -- `(0, 0)` for any other macro, and
/// (without the `verus_ir` feature) for `verus!` too, matching this
/// scanner's existing best-effort posture: a missed name only risks
/// under-counting a module's real leaf-name total, never a false thin-
/// module report. `syn::visit::Visit` never descends into a macro's own
/// token stream, and Verus's grammar extensions (`spec fn`/`open`/
/// `closed`/`requires`/`ensures`) aren't parseable by plain `syn` even if
/// something did -- see `crate::verus_ir::count_verus_item_names`'s own
/// doc comment for the real `verus_syn`-based parse this delegates to.
#[cfg(feature = "verus_ir")]
#[instrument(level = "debug", skip(item_macro))]
fn verus_macro_leaf_names(item_macro: &ItemMacro) -> (usize, usize) {
    if !item_macro.mac.path.is_ident("verus") {
        return (0, 0);
    }
    crate::verus_ir::count_verus_item_names(item_macro.mac.tokens.clone())
}

#[cfg(not(feature = "verus_ir"))]
#[instrument(level = "debug", skip(_item_macro))]
fn verus_macro_leaf_names(_item_macro: &ItemMacro) -> (usize, usize) {
    (0, 0)
}

#[instrument(level = "debug", skip(node))]
pub(super) fn public_path_mods(node: &ModuleNode) -> Vec<&ModuleNode> {
    let mut out = Vec::new();
    for child in &node.children {
        if child.declared_vis.is_unrestricted_pub() && child.ancestors_all_pub {
            out.push(child);
            out.extend(public_path_mods(child));
        }
    }
    out
}
