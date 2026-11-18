//! Which files a crate's `mod` declarations lead to, as queries.
//!
//! Finding the crate's files from its entry file reads the syntax of each for its out-of-line `mod`
//! declarations, and then asks which of the files they may lead to exists. The first is a function of
//! the file's syntax and is kept with the file; the second is a question of the files the database
//! holds, answered from the file set of the source root of the file that declares them (an input
//! of the database), so that a file that appears or goes changes what read it. The files of the crate
//! are then followed from the entry through those two, with no syntax and no file system.

use std::path::{Component, Path, PathBuf};

use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::SourceDatabase as RaSourceDatabase;
use ra_ap_vfs::{FileId, VfsPath};
use tracing::instrument;

use super::crate_files::{FileKey, carried};
use super::queries::SemanticDatabase;
use crate::loader::native_syntax::{ModuleTarget, declared_modules};

/// The files a file's out-of-line `mod` declarations may lead to, from its syntax alone.
#[salsa::tracked(persist, returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn module_declarations_file<'db>(
    db: &'db dyn SemanticDatabase,
    key: FileKey<'db>,
) -> Option<Vec<ModuleTarget>> {
    use ra_ap_syntax::ast::HasModuleItem;
    carried(db, key, "module_declarations_file", |source| {
        Ok(declared_modules(source.tree().items(), source.path()))
    })
}

/// The crate's entry file: the root of the tree of `mod` declarations.
#[salsa::interned(persist)]
pub(super) struct ModuleTreeKey<'db> {
    #[returns(copy)]
    pub(super) entry: FileKey<'db>,
}

/// `path` without its `.` and `..` components, which need no file system to remove.
fn normalized(path: &Path) -> PathBuf {
    let mut kept = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                kept.pop();
            }
            other => kept.push(other.as_os_str()),
        }
    }
    kept
}

/// The file at `path`, if the source root of `from` holds it: what a read of that root's file set
/// depends on.
fn held_file(db: &RootDatabase, from: FileId, path: &Path) -> Option<FileId> {
    let root = db.file_source_root(from).source_root_id(db);
    let files = db.source_root(root).source_root(db);
    let path = VfsPath::new_real_path(path.to_str()?.to_owned());
    files.file_for_path(&path).copied()
}

/// The file a declaration leads to, among those the database holds. `None` where there is none, and
/// an error where the declaration cannot be read.
fn followed(
    db: &RootDatabase,
    from: FileId,
    target: &ModuleTarget,
) -> Result<Option<(FileId, PathBuf)>, ()> {
    let held = |path: &Path| {
        let path = normalized(path);
        held_file(db, from, &path).map(|file| (file, path))
    };
    match target {
        ModuleTarget::Explicit(path) => Ok(held(path)),
        ModuleTarget::Invalid => Err(()),
        ModuleTarget::Default { flat, nested } => Ok(held(flat).or_else(|| held(nested))),
        ModuleTarget::Nowhere => Ok(None),
    }
}

/// The files of the crate, from its entry file by the `mod` declarations, sorted. `None` where a file
/// cannot be read, so that the caller reads it the way that says why.
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn module_tree<'db>(
    db: &'db dyn SemanticDatabase,
    key: ModuleTreeKey<'db>,
) -> Option<Vec<PathBuf>> {
    let entry = key.entry(db);
    let (root, tree) = (entry.root(db), entry.tree(db));
    let root_db = db.root_database();
    tracing::debug!(target: "cordial::semantic::queries", function = %root.display(), query = "module_tree", "semantic query executed");
    let mut seen = std::collections::BTreeSet::new();
    let mut queue = vec![(entry.file(db), entry.path(db).clone())];
    while let Some((file, path)) = queue.pop() {
        if crate::loader::path_has_fixtures(&path, root) || !seen.insert(path.clone()) {
            continue;
        }
        let here = FileKey::new(db, file, path.clone(), tree.clone(), root.clone());
        for target in module_declarations_file(db, here).as_ref()? {
            if let Some((child, child_path)) = followed(root_db, file, target).ok()? {
                queue.push((child, child_path));
            }
        }
    }
    Some(seen.into_iter().collect())
}
