//! syn-based scan for glob `use` trees.

use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{File, ItemMod, ItemUse, UseTree};

use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures, quality_scan_trees};

use super::types::{GlobImportRuleId, GlobImportSiteRecord};

use tracing::instrument;

/// Scan one crate for glob imports.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_glob_imports(crate_root: &Path) -> CordialResult<Vec<GlobImportSiteRecord>> {
    let mut findings = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        findings.extend(scan_source_tree(&tree_root, crate_root)?);
    }

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.snippet.cmp(&b.snippet))
    });

    Ok(findings)
}

#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<GlobImportSiteRecord>> {
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
) -> CordialResult<Vec<GlobImportSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(tree_root, file);
    let mut visitor = GlobImportVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        findings: Vec::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.findings)
}

struct GlobImportVisitor {
    file: PathBuf,
    crate_root: PathBuf,
    module_prefix: Vec<String>,
    findings: Vec<GlobImportSiteRecord>,
}

impl GlobImportVisitor {
    #[instrument(level = "debug", skip(self))]
    fn site_context(&self) -> String {
        if self.module_prefix.is_empty() {
            "<crate>".to_string()
        } else {
            self.module_prefix.join("::")
        }
    }

    #[instrument(level = "debug", skip(self, tree))]
    fn walk_use_tree(&mut self, tree: &UseTree, prefix: &mut Vec<String>) {
        match tree {
            UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.walk_use_tree(&path.tree, prefix);
                prefix.pop();
            }
            UseTree::Group(group) => {
                for item in &group.items {
                    self.walk_use_tree(item, prefix);
                }
            }
            UseTree::Glob(glob) => {
                if prefix.last().is_some_and(|segment| segment == "prelude") {
                    // `use <path>::prelude::*;` -- a crate's own `prelude`
                    // module is conventionally designed to be glob-imported
                    // (the same way `std::prelude::*` is auto-imported into
                    // every ordinary Rust crate): `vstd::prelude`,
                    // `itertools::prelude`, `rayon::prelude`, `diesel::
                    // prelude`, and every other crate shipping one all
                    // follow this. Confirmed real, not just convenient: a
                    // Verus proof file's `use vstd::prelude::*;` brings in
                    // the ghost/tracked machinery and internal names the
                    // `verus! { .. }` macro's own expansion depends on --
                    // there is no idiomatic explicit-list alternative, the
                    // same way there's no explicit-list alternative to
                    // `std`'s own implicit prelude.
                    return;
                }
                let snippet = if prefix.is_empty() {
                    "*".to_string()
                } else {
                    format!("{}::*", prefix.join("::"))
                };
                let mut file = self.file.clone();
                if let Ok(rel) = file.strip_prefix(&self.crate_root) {
                    file = rel.to_path_buf();
                }
                self.findings.push(GlobImportSiteRecord {
                    rule_id: GlobImportRuleId::Import001,
                    context: self.site_context(),
                    file,
                    line: glob.span().start().line as u32,
                    snippet,
                });
            }
            UseTree::Name(_) | UseTree::Rename(_) => {}
        }
    }
}

impl<'ast> Visit<'ast> for GlobImportVisitor {
    #[instrument(level = "debug", skip(self, node))]
    fn visit_file(&mut self, node: &'ast File) {
        syn::visit::visit_file(self, node);
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        let Some((_, items)) = &node.content else {
            syn::visit::visit_item_mod(self, node);
            return;
        };
        let prev = self.module_prefix.clone();
        self.module_prefix.push(node.ident.to_string());
        for item in items {
            syn::visit::visit_item(self, item);
        }
        self.module_prefix = prev;
    }

    #[instrument(level = "debug", skip(self, node))]
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let mut prefix = Vec::new();
        self.walk_use_tree(&node.tree, &mut prefix);
    }
}
