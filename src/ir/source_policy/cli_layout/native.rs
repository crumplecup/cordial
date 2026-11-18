//! Native CLI projection: canonical database syntax, with no parser fallback.
mod act_shape;
mod collect;
mod dispatch;
mod syntax;
use super::{
    CliLayoutFacts, EntryItem,
    catalog::{ActRec, FreeFnRec, LayoutCatalog, TypeRec},
};
use crate::{
    CordialResult, FileSnapshot,
    loader::{DatabaseSource, ModuleTarget, path_has_fixtures},
};
use act_shape::ActNode;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// What one file contributes to the crate's layout, before the types of every file are known: plain
/// data, so that it is saved with the database. The bodies of the `act` functions are kept as data
/// (see [`act_shape`]), and the files its `mod` declarations lead to as what they may be.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_new::new)]
pub(crate) struct NativeFile {
    types: Vec<TypeRec>,
    error_impls: BTreeSet<String>,
    free_fns: Vec<FreeFnRec>,
    acts: Vec<(String, PathBuf, u32, ActNode)>,
    entries: Vec<EntryItem>,
    children: Vec<ModuleTarget>,
    snapshot: FileSnapshot,
}

/// The questions the crate's collection asks of the files, which the loader answers from what the
/// database holds.
pub(crate) trait LayoutFiles {
    /// What the file at `path` contributes.
    fn read(&mut self, path: &Path) -> CordialResult<NativeFile>;
    /// The file a declaration in `from` leads to, if the database holds it.
    fn follow(&mut self, from: &Path, target: &ModuleTarget) -> CordialResult<Option<PathBuf>>;
    /// Whether the database holds the file at `path`.
    fn holds(&mut self, path: &Path) -> bool;
    /// The Rust files the database holds under `dir`, sorted.
    fn under(&mut self, dir: &Path) -> Vec<PathBuf>;
}
impl super::super::SourcePolicyInventory {
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    #[hotpath::measure]
    pub(crate) fn collect_native_cli_layout(
        root: &Path,
        name: &str,
        source: &mut impl LayoutFiles,
    ) -> CordialResult<CliLayoutFacts> {
        let src = root.join("src");
        let lib = src.join("lib.rs");
        let main = src.join("main.rs");
        let bins = src.join("bin");
        let binaries = source.under(&bins);
        if !source.holds(&lib) || (!source.holds(&main) && binaries.is_empty()) {
            return Ok(CliLayoutFacts::default());
        }
        let mut files = BTreeMap::new();
        let mut library = BTreeSet::new();
        let mut binary = BTreeSet::new();
        let mut entries = Vec::new();
        tree(&lib, root, &mut library, &mut files, source)?;
        if source.holds(&main) {
            tree(&main, root, &mut binary, &mut files, source)?;
            entries.push(main);
        }
        for path in binaries {
            if path_has_fixtures(&path, root) {
                continue;
            }
            tree(&path, root, &mut binary, &mut files, source)?;
            if path.parent() == Some(bins.as_path()) {
                entries.push(path);
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
    #[hotpath::measure]
    pub(crate) fn native_cli_layout_file(
        file: &DatabaseSource<'_>,
        name: &str,
    ) -> CordialResult<NativeFile> {
        collect::file(file, name)
    }
}
#[tracing::instrument(level = "debug", skip(seen, files, source, path), err(level = "warn"))]
#[hotpath::measure]
fn tree(
    path: &Path,
    root: &Path,
    seen: &mut BTreeSet<PathBuf>,
    files: &mut BTreeMap<PathBuf, NativeFile>,
    source: &mut impl LayoutFiles,
) -> CordialResult<()> {
    if path_has_fixtures(path, root) || !seen.insert(path.to_path_buf()) {
        return Ok(());
    }
    if !files.contains_key(path) {
        files.insert(path.to_path_buf(), source.read(path)?);
    }
    let targets = files
        .get(path)
        .ok_or_else(|| crate::CordialError::not_found(path.to_path_buf()))?
        .children
        .clone();
    for target in targets {
        if let Some(child) = source.follow(path, &target)? {
            tree(&child, root, seen, files, source)?;
        }
    }
    Ok(())
}
