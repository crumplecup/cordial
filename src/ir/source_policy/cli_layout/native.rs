//! Native CLI projection: canonical database syntax, with no parser fallback.
mod collect;
mod dispatch;
mod syntax;
use super::{
    CliLayoutFacts, EntryItem,
    catalog::{ActRec, FreeFnRec, LayoutCatalog, TypeRec},
};
use crate::{
    CordialResult, FileSnapshot,
    loader::{DatabaseSource, path_has_fixtures},
};
use ra_ap_syntax::ast;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// Transient loader assembly; syntax never crosses the shared-IR boundary.
#[derive(derive_new::new)]
pub(crate) struct NativeFile {
    types: Vec<TypeRec>,
    error_impls: BTreeSet<String>,
    free_fns: Vec<FreeFnRec>,
    acts: Vec<(String, PathBuf, u32, ast::BlockExpr)>,
    entries: Vec<EntryItem>,
    children: Vec<PathBuf>,
    snapshot: FileSnapshot,
}
impl super::super::SourcePolicyInventory {
    #[tracing::instrument(level = "debug", skip(read), err(level = "warn"))]
    pub(crate) fn collect_native_cli_layout(
        root: &Path,
        name: &str,
        mut read: impl FnMut(&Path) -> CordialResult<NativeFile>,
    ) -> CordialResult<CliLayoutFacts> {
        let src = root.join("src");
        let lib = src.join("lib.rs");
        let main = src.join("main.rs");
        let bins = src.join("bin");
        if !lib.is_file() || (!main.is_file() && !bins.is_dir()) {
            return Ok(CliLayoutFacts::default());
        }
        let mut files = BTreeMap::new();
        let mut library = BTreeSet::new();
        let mut binary = BTreeSet::new();
        let mut entries = Vec::new();
        tree(&lib, root, &mut library, &mut files, &mut read)?;
        if main.is_file() {
            tree(&main, root, &mut binary, &mut files, &mut read)?;
            entries.push(main);
        }
        if bins.is_dir() {
            for entry in walkdir::WalkDir::new(&bins)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
            {
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "rs") || path_has_fixtures(path, root) {
                    continue;
                }
                tree(path, root, &mut binary, &mut files, &mut read)?;
                if path.parent() == Some(bins.as_path()) {
                    entries.push(path.to_path_buf());
                }
            }
        }
        let mut catalog = LayoutCatalog::new(name.into());
        let mut pending = Vec::new();
        let mut snapshots = Vec::new();
        let mut entry_items = Vec::new();
        for path in library.iter().chain(binary.difference(&library)) {
            let file = files
                .remove(path)
                .ok_or_else(|| crate::CordialError::not_found(path.clone()))?;
            let in_library = library.contains(path);
            for item in file.types {
                let mut item = item;
                item.merge(
                    false,
                    false,
                    false,
                    BTreeMap::new(),
                    BTreeMap::new(),
                    in_library,
                );
                if let Some(existing) = catalog.types_mut().get_mut(item.ident()) {
                    existing.merge(
                        item.parser(),
                        item.subcommand(),
                        item.error(),
                        item.fields().clone(),
                        item.variants().clone(),
                        in_library,
                    );
                } else {
                    catalog.insert_type(item);
                }
            }
            for ident in file.error_impls {
                if let Some(item) = catalog.types_mut().get_mut(&ident) {
                    item.mark_error();
                }
            }
            for item in file.free_fns {
                catalog.add_free_fn(FreeFnRec::new(
                    item.name().clone(),
                    item.file().clone(),
                    item.line(),
                    in_library,
                    item.input_idents().clone(),
                ));
            }
            pending.extend(file.acts);
            if entries.contains(path) {
                entry_items.extend(file.entries);
            }
            snapshots.push(file.snapshot);
        }
        for (ident, file, line, body) in pending {
            let targets = dispatch::targets(&catalog, &ident, &body);
            catalog
                .acts_mut()
                .insert(ident, ActRec::new(file, line, targets));
        }
        // Preserve main-first and binary discovery order, independent of catalog sorting.
        entry_items.sort_by(|a, b| {
            entries
                .iter()
                .position(|p| p == a.file())
                .cmp(&entries.iter().position(|p| p == b.file()))
                .then(a.line().cmp(&b.line()))
        });
        snapshots.sort_by(|a, b| a.path().cmp(b.path()));
        Ok(CliLayoutFacts::new(catalog, entry_items, snapshots))
    }
    #[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
    pub(crate) fn native_cli_layout_file(
        file: &DatabaseSource<'_>,
        name: &str,
    ) -> CordialResult<NativeFile> {
        collect::file(file, name)
    }
}
#[tracing::instrument(level = "debug", skip(seen, files, read, path), err(level = "warn"))]
fn tree(
    path: &Path,
    root: &Path,
    seen: &mut BTreeSet<PathBuf>,
    files: &mut BTreeMap<PathBuf, NativeFile>,
    read: &mut impl FnMut(&Path) -> CordialResult<NativeFile>,
) -> CordialResult<()> {
    if path_has_fixtures(path, root) || !seen.insert(path.to_path_buf()) {
        return Ok(());
    }
    if !files.contains_key(path) {
        files.insert(path.to_path_buf(), read(path)?);
    }
    let children = files
        .get(path)
        .ok_or_else(|| crate::CordialError::not_found(path.to_path_buf()))?
        .children
        .clone();
    for child in children {
        tree(&child, root, seen, files, read)?;
    }
    Ok(())
}
