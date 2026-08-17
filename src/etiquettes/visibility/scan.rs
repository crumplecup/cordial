//! Crate-tree scan for `pub mod` paths that do not earn their visibility.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use syn::spanned::Spanned;
use syn::{Attribute, Item, ItemMod, UseTree, Visibility};

use crate::error::CordialResult;

use super::types::{VisibilityRecord, VisibilityRuleId, VisibilityThresholds};

/// Cached branching floor from a previous peel. Digest is a hash of the
/// scanned crate files; a source edit invalidates it and forces a re-peel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchingCache {
    pub digest: String,
    pub floor: usize,
}

impl BranchingCache {
    pub fn load(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn write(&self, path: &Path) -> CordialResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// How the scanner applies [`VisibilityThresholds::min_module_names`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisibilityEval {
    /// `prefer_root`: thin checks use `min_module_names`. An oversized flat
    /// root is accepted — that is the preferred resolution.
    Normal,
    /// `prefer_root = false`: thin checks use `floor`, which starts at
    /// `min_module_names` and drops as the largest undersized modules are
    /// peeled back off root.
    Branching { floor: usize },
}

impl VisibilityEval {
    fn thin_floor(self, thresholds: VisibilityThresholds) -> usize {
        match self {
            Self::Normal => thresholds.min_module_names,
            Self::Branching { floor } => floor,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisKind {
    Private,
    Pub,
    PubCrate,
    PubSuper,
}

impl VisKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Pub => "pub",
            Self::PubCrate => "pub(crate)",
            Self::PubSuper => "pub(super)",
        }
    }

    fn is_unrestricted_pub(self) -> bool {
        matches!(self, Self::Pub)
    }
}

struct ModuleNode {
    path: String,
    file: PathBuf,
    line: u32,
    declared_vis: VisKind,
    parent_declared_pub: bool,
    ancestors_all_pub: bool,
    is_crate_root: bool,
    leaf_pub: usize,
    leaf_crate: usize,
    children: Vec<ModuleNode>,
}

/// Walk the crate's root module tree (`src/lib.rs` or `src/main.rs`) and
/// apply `thresholds`. The scanner never picks thresholds itself.
#[tracing::instrument]
pub fn scan_crate_visibility(
    crate_root: &Path,
    thresholds: VisibilityThresholds,
) -> CordialResult<Vec<VisibilityRecord>> {
    Ok(scan_crate_visibility_with_cache(crate_root, thresholds, None)?.0)
}

/// Same as [`scan_crate_visibility`], but reuses a branching floor when the
/// crate-file digest still matches. On mismatch (or first run) this peels,
/// writes a new cache payload, then applies the lowered floor — a two-pass
/// analysis so undersized peeled modules do not fire `VIS-MOD-THIN-001`.
#[tracing::instrument(skip(cached))]
pub fn scan_crate_visibility_with_cache(
    crate_root: &Path,
    thresholds: VisibilityThresholds,
    cached: Option<BranchingCache>,
) -> CordialResult<(Vec<VisibilityRecord>, Option<BranchingCache>)> {
    let Some(root_file) = crate_root_file(crate_root) else {
        return Ok((Vec::new(), None));
    };
    let root = scan_module_file(
        &root_file,
        "crate".to_string(),
        VisKind::Pub,
        true,
        true,
        true,
    )?;
    let (eval, new_cache) = resolve_eval(&root, thresholds, cached);
    Ok((collect_findings(&root, thresholds, eval), new_cache))
}

fn crate_root_file(crate_root: &Path) -> Option<PathBuf> {
    let src_lib = crate_root.join("src").join("lib.rs");
    if src_lib.is_file() {
        return Some(src_lib);
    }
    let src_main = crate_root.join("src").join("main.rs");
    if src_main.is_file() {
        return Some(src_main);
    }
    let lib = crate_root.join("lib.rs");
    if lib.is_file() {
        return Some(lib);
    }
    None
}

struct ScanHeader<'a> {
    file: &'a Path,
    path: String,
    declared_vis: VisKind,
    parent_declared_pub: bool,
    ancestors_all_pub: bool,
    is_crate_root: bool,
    line: u32,
}

fn scan_module_file(
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

fn collect_findings(
    root: &ModuleNode,
    thresholds: VisibilityThresholds,
    eval: VisibilityEval,
) -> Vec<VisibilityRecord> {
    let external = external_name_count(root);
    let mut out = Vec::new();
    if external < thresholds.max_crate_names_for_flat {
        for pub_mod in public_path_mods(root) {
            out.push(VisibilityRecord {
                rule_id: VisibilityRuleId::CrateFlat001,
                module_path: pub_mod.path.clone(),
                file: pub_mod.file.clone(),
                line: pub_mod.line,
                name_count: external,
                parent_vis: "pub".to_string(),
                declared_vis: pub_mod.declared_vis.as_str().to_string(),
            });
        }
    }
    let thin_floor = eval.thin_floor(thresholds);
    collect_module_findings(root, thin_floor, &mut out);
    out.sort_by(|a, b| {
        a.rule_id
            .as_str()
            .cmp(b.rule_id.as_str())
            .then_with(|| a.module_path.cmp(&b.module_path))
    });
    out
}

fn collect_module_findings(node: &ModuleNode, thin_floor: usize, out: &mut Vec<VisibilityRecord>) {
    if !node.is_crate_root {
        let mismatch = node.declared_vis.is_unrestricted_pub() && !node.parent_declared_pub;
        if mismatch {
            out.push(VisibilityRecord {
                rule_id: VisibilityRuleId::ModMismatch001,
                module_path: node.path.clone(),
                file: node.file.clone(),
                line: node.line,
                name_count: node.leaf_crate,
                parent_vis: if node.parent_declared_pub {
                    "pub".to_string()
                } else {
                    "non-pub".to_string()
                },
                declared_vis: node.declared_vis.as_str().to_string(),
            });
        }
        let is_path = node.declared_vis.is_unrestricted_pub()
            || node.declared_vis == VisKind::PubCrate
            || mismatch;
        if is_path {
            let count = if node.declared_vis.is_unrestricted_pub() && node.ancestors_all_pub {
                node.leaf_pub
            } else {
                node.leaf_crate
            };
            if count < thin_floor {
                out.push(VisibilityRecord {
                    rule_id: VisibilityRuleId::ModThin001,
                    module_path: node.path.clone(),
                    file: node.file.clone(),
                    line: node.line,
                    name_count: count,
                    parent_vis: if node.parent_declared_pub {
                        "pub".to_string()
                    } else {
                        "non-pub".to_string()
                    },
                    declared_vis: node.declared_vis.as_str().to_string(),
                });
            }
        }
    }
    for child in &node.children {
        collect_module_findings(child, thin_floor, out);
    }
}

fn resolve_eval(
    root: &ModuleNode,
    thresholds: VisibilityThresholds,
    cached: Option<BranchingCache>,
) -> (VisibilityEval, Option<BranchingCache>) {
    if thresholds.prefer_root {
        return (VisibilityEval::Normal, None);
    }
    let digest = tree_digest(root);
    if let Some(cache) = cached.filter(|cache| cache.digest == digest) {
        return (
            VisibilityEval::Branching { floor: cache.floor },
            Some(cache),
        );
    }
    let floor = peel_branching_floor(root, thresholds);
    let cache = BranchingCache { digest, floor };
    (VisibilityEval::Branching { floor }, Some(cache))
}

/// Peel the largest undersized public-path modules off a conceptually
/// flattened root until remaining names sit under `max_crate_names_for_flat`.
/// Modules that already meet `min_module_names` stay put and do not move the
/// floor. The thin floor follows each peeled module's size (10 → 9 → 7 → 6).
fn peel_branching_floor(root: &ModuleNode, thresholds: VisibilityThresholds) -> usize {
    let mut floor = thresholds.min_module_names;
    let mods = public_path_mods(root);
    let mut remaining = external_name_count(root);
    let mut reserved: Vec<&str> = Vec::new();
    for module in &mods {
        let size = external_name_count(module);
        if size < thresholds.min_module_names {
            continue;
        }
        if reserved
            .iter()
            .any(|parent| is_path_under(&module.path, parent))
        {
            continue;
        }
        remaining = remaining.saturating_sub(size);
        reserved.push(&module.path);
    }
    if remaining < thresholds.max_crate_names_for_flat {
        return floor;
    }
    let mut candidates: Vec<&ModuleNode> = mods
        .iter()
        .copied()
        .filter(|module| {
            let size = external_name_count(module);
            size > 0
                && size < thresholds.min_module_names
                && !reserved
                    .iter()
                    .any(|parent| is_path_under(&module.path, parent))
        })
        .collect();
    candidates.sort_by(|left, right| {
        external_name_count(right)
            .cmp(&external_name_count(left))
            .then_with(|| left.path.cmp(&right.path))
    });
    for candidate in candidates {
        if remaining < thresholds.max_crate_names_for_flat {
            break;
        }
        if reserved
            .iter()
            .any(|parent| is_path_under(&candidate.path, parent))
        {
            continue;
        }
        let size = external_name_count(candidate);
        remaining = remaining.saturating_sub(size);
        floor = size;
        reserved.push(&candidate.path);
    }
    floor
}

fn is_path_under(path: &str, parent: &str) -> bool {
    path == parent || path.starts_with(&format!("{parent}::"))
}

fn tree_digest(root: &ModuleNode) -> String {
    let mut files = Vec::new();
    collect_files(root, &mut files);
    files.sort();
    files.dedup();
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.to_string_lossy().as_bytes());
        if let Ok(bytes) = std::fs::read(&file) {
            hasher.update(&bytes);
        }
    }
    format!("{:x}", hasher.finalize())
}

fn collect_files(node: &ModuleNode, files: &mut Vec<PathBuf>) {
    files.push(node.file.clone());
    for child in &node.children {
        collect_files(child, files);
    }
}

fn external_name_count(node: &ModuleNode) -> usize {
    let mut n = node.leaf_pub;
    for child in &node.children {
        if child.declared_vis.is_unrestricted_pub() {
            n += external_name_count(child);
        }
    }
    n
}

fn public_path_mods(node: &ModuleNode) -> Vec<&ModuleNode> {
    let mut out = Vec::new();
    for child in &node.children {
        if child.declared_vis.is_unrestricted_pub() && child.ancestors_all_pub {
            out.push(child);
            out.extend(public_path_mods(child));
        }
    }
    out
}

fn vis_kind(vis: &Visibility) -> VisKind {
    match vis {
        Visibility::Public(_) => VisKind::Pub,
        Visibility::Inherited => VisKind::Private,
        Visibility::Restricted(restricted) => {
            if restricted.path.is_ident("crate") {
                VisKind::PubCrate
            } else if restricted.path.is_ident("super") {
                VisKind::PubSuper
            } else {
                VisKind::PubCrate
            }
        }
    }
}

fn item_vis(item: &Item) -> Option<VisKind> {
    let vis = match item {
        Item::Const(item) => &item.vis,
        Item::Enum(item) => &item.vis,
        Item::Fn(item) => &item.vis,
        Item::Static(item) => &item.vis,
        Item::Struct(item) => &item.vis,
        Item::Trait(item) => &item.vis,
        Item::TraitAlias(item) => &item.vis,
        Item::Type(item) => &item.vis,
        Item::Use(item) => &item.vis,
        Item::Union(item) => &item.vis,
        Item::ForeignMod(_) | Item::Impl(_) | Item::Macro(_) | Item::Verbatim(_) | Item::Mod(_) => {
            return None;
        }
        _ => return None,
    };
    Some(vis_kind(vis))
}

fn leaf_name_count(item: &Item) -> usize {
    match item {
        Item::Use(item) => use_name_count(&item.tree),
        Item::Const(_)
        | Item::Enum(_)
        | Item::Fn(_)
        | Item::Static(_)
        | Item::Struct(_)
        | Item::Trait(_)
        | Item::TraitAlias(_)
        | Item::Type(_)
        | Item::Union(_) => 1,
        _ => 0,
    }
}

fn use_name_count(tree: &UseTree) -> usize {
    match tree {
        UseTree::Name(_) | UseTree::Rename(_) => 1,
        UseTree::Glob(_) => 0,
        UseTree::Path(path) => use_name_count(&path.tree),
        UseTree::Group(group) => group.items.iter().map(use_name_count).sum(),
    }
}

fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        list.path.is_ident("cfg") && list.tokens.to_string().replace(' ', "") == "test"
    })
}

fn is_doc_hidden(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("doc") {
            return false;
        }
        match &attr.meta {
            syn::Meta::List(list) => list.tokens.to_string().replace(' ', "").contains("hidden"),
            _ => false,
        }
    })
}
