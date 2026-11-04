//! Shipping syntax projection at the shared loader boundary.
mod catalog;
mod hunt;
mod idents;
mod tree;
use super::{CliLayoutFacts, EntryItem};
use crate::{
    CordialResult, FileSnapshot,
    loader::{SourceLoadView, path_has_fixtures},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use syn::{Item, spanned::Spanned, visit::Visit};
use tracing::instrument;
use tree::collect_mod_tree;
#[instrument(level = "debug", skip(source), err(level = "warn"))]
pub(super) fn collect(
    source: &SourceLoadView,
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<CliLayoutFacts> {
    let src_root = crate_root.join("src");
    let lib = src_root.join("lib.rs");
    if !lib.is_file() {
        return Ok(CliLayoutFacts::default());
    }
    let main = src_root.join("main.rs");
    let bin_dir = src_root.join("bin");
    if !main.is_file() && !bin_dir.is_dir() {
        return Ok(CliLayoutFacts::default());
    }

    let lib_files = collect_tree(source, crate_root, &lib)?;
    let mut bin_files = BTreeSet::new();
    let mut entry_files = Vec::new();
    if main.is_file() {
        bin_files.extend(collect_tree(source, crate_root, &main)?);
        entry_files.push(main.clone());
    }
    if bin_dir.is_dir() {
        for path in rust_files_in(&bin_dir, crate_root)? {
            bin_files.extend(collect_tree(source, crate_root, &path)?);
            if path.parent() == Some(bin_dir.as_path()) {
                entry_files.push(path);
            }
        }
    }
    let bin_only: BTreeSet<PathBuf> = bin_files.difference(&lib_files).cloned().collect();

    let mut catalog = catalog::LayoutCatalog::new(crate_name.to_string());
    for path in lib_files.iter().chain(bin_only.iter()) {
        catalog::load_shared_file(
            &mut catalog,
            &source.parsed_file(path, "cli-layout-inventory")?,
            lib_files.contains(path),
        )?;
    }
    hunt::finalize_acts(&mut catalog);

    let mut entries = Vec::new();
    for path in &entry_files {
        let file = source.parsed_file(path, "cli-layout-inventory")?;
        entries.extend(collect_entries(file.path(), file.syntax()));
    }
    let mut snapshots = Vec::new();
    for path in lib_files.union(&bin_files) {
        let file = source.parsed_file(path, "cli-layout-inventory")?;
        snapshots.push(FileSnapshot::new(
            file.path().to_path_buf(),
            file.source_digest(),
        ));
    }
    Ok(CliLayoutFacts::new(
        catalog.into_resolved(),
        entries,
        snapshots,
    ))
}
#[instrument(level = "debug", skip(source), err(level = "warn"))]
fn collect_tree(
    source: &SourceLoadView,
    crate_root: &Path,
    root: &Path,
) -> CordialResult<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    collect_mod_tree(source, root, crate_root, &mut files)?;
    Ok(files)
}

#[instrument(level = "debug", err(level = "warn"))]
fn rust_files_in(dir: &Path, crate_root: &Path) -> CordialResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
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
    Ok(files)
}

#[instrument(level = "debug", skip(syntax, file))]
fn collect_entries(file: &Path, syntax: &syn::File) -> Vec<EntryItem> {
    use super::EntryKind;
    syntax
        .items
        .iter()
        .filter_map(|item| {
            let (name, kind) = match item {
                Item::Fn(f) if f.sig.ident == "main" => {
                    let mut hunt = MainHunt {
                        has_match: false,
                        has_parse: false,
                        has_act: false,
                    };
                    hunt.visit_block(&f.block);
                    (
                        "main".into(),
                        EntryKind::Main {
                            has_match: hunt.has_match,
                            has_parse: hunt.has_parse,
                            has_act: hunt.has_act,
                        },
                    )
                }
                Item::Fn(f) => (f.sig.ident.to_string(), EntryKind::Function),
                Item::Mod(m) => (m.ident.to_string(), EntryKind::Module),
                Item::Struct(s) => (s.ident.to_string(), EntryKind::Struct),
                Item::Enum(e) => (e.ident.to_string(), EntryKind::Enum),
                Item::Impl(_) => (String::new(), EntryKind::Impl),
                _ => return None,
            };
            Some(EntryItem::new(
                file.to_path_buf(),
                item.span().start().line as u32,
                name,
                kind,
            ))
        })
        .collect()
}
struct MainHunt {
    has_match: bool,
    has_parse: bool,
    has_act: bool,
}
impl<'ast> Visit<'ast> for MainHunt {
    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        self.has_match = true;
        syn::visit::visit_expr_match(self, node);
    }
    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.has_parse |= node.method == "parse";
        self.has_act |= node.method == "act";
        syn::visit::visit_expr_method_call(self, node);
    }
    #[instrument(level = "trace", skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = node.func.as_ref()
            && let Some(last) = path.path.segments.last()
        {
            self.has_parse |= last.ident == "parse";
            self.has_act |= last.ident == "act";
        }
        syn::visit::visit_expr_call(self, node);
    }
}
