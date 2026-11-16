//! Compiled artifacts depend on HIR crate and module identities, not folders.

use super::super::item_key;
use crate::SemanticConfigurationInput;
use ra_ap_hir::Crate;
use ra_ap_ide_db::{RootDatabase, base_db::SourceDatabase as RaSourceDatabase};
use ra_ap_vfs::Vfs;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

/// Retain every reachable Rust module that must be rebuilt before querying
/// a different build script or proc-macro implementation. Toolchain libraries
/// are precompiled inputs; their source files do not build these artifacts.
#[tracing::instrument(level = "debug", skip_all)]
#[hotpath::measure]
pub(super) fn collect(
    db: &RootDatabase,
    vfs: &Vfs,
    build_scripts: &[PathBuf],
) -> Vec<SemanticConfigurationInput> {
    let mut pending: Vec<_> = Crate::all(db)
        .into_iter()
        .filter(|krate| {
            krate.base().data(db).is_proc_macro
                || vfs
                    .file_path(krate.root_file(db))
                    .as_path()
                    .is_some_and(|root| {
                        build_scripts
                            .iter()
                            .any(|path| path == AsRef::<std::path::Path>::as_ref(root))
                    })
        })
        .collect();
    let mut visited = HashSet::new();
    let mut inputs = BTreeMap::new();
    while let Some(krate) = pending.pop() {
        if krate.is_builtin(db) || !visited.insert(krate) {
            continue;
        }
        pending.extend(krate.dependencies(db).into_iter().map(|dep| dep.krate));
        // The saved lists of the crate's files, which make no definition map to answer.
        let listed = item_key::files_of(db, krate)
            .zip(item_key::included_files_of(db, krate))
            .map(|(modules, included)| modules.into_iter().chain(included).collect::<Vec<_>>());
        let files = listed.unwrap_or_else(|| {
            let modules = krate.modules(db).into_iter().map(|module| {
                module
                    .definition_source_file_id(db)
                    .original_file_respecting_includes(db)
                    .file_id(db)
            });
            let includes = ra_ap_hir_def::include_macro_invoc(db, krate.base())
                .iter()
                .map(|(_, file)| file.file_id(db));
            modules.chain(includes).collect()
        });
        for file in files {
            if let Some(path) = vfs.file_path(file).as_path() {
                inputs.insert(
                    AsRef::<std::path::Path>::as_ref(path).to_path_buf(),
                    Sha256::digest(db.file_text(file).text(db).as_bytes()).into(),
                );
            }
        }
    }
    inputs
        .into_iter()
        .map(|(path, digest)| SemanticConfigurationInput::new(path, Some(digest)))
        .collect()
}
