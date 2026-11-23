//! Contexts that differ in nothing a crate names share that crate's copy.
//!
//! A crate's copy under a context is its original under the cfg the context gives it, and what that
//! cfg does to the crate is what the crate's own files say of it: the atoms their `cfg` and `cfg_attr`
//! predicates name. Two contexts that agree on those atoms give a crate the same copy, so the second
//! takes the first's: the same crate, with its definition map and everything computed from it. The same
//! holds up the dependency graph: a crate's copy is the same only if the copies it depends on are.
//!
//! A crate with a file that reads the cfg some other way (`cfg!`, `include!`, `#[path]`) names more than
//! its predicates say, so it takes the whole context as what it differs in, and shares with the contexts
//! that are the same in every atom. That is the assumption the file-local rule of the harvest already
//! makes: a file whose gates the context does not flip is as it was.
//!
//! A copy is kept, and its memos with it, until the pass over the contexts ends (see
//! [`SourceDatabase::release_shared_copies`]), since the next context may take it. Under memory pressure it
//! is given up with the context that made it.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use ra_ap_ide_db::base_db::Crate as InputCrate;
use ra_ap_vfs::FileId;
use tracing::instrument;

use super::SourceDatabase;
use super::contexts::atoms_of;
use crate::SemanticContextKey;

/// What two copies of a crate must have in common to be one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct CopyKey {
    krate: InputCrate,
    enabled: Vec<String>,
    disabled: Vec<String>,
    /// The crates it depends on, as the copy sees them: the copy of one that is copied, the crate itself
    /// when it is not.
    below: Vec<InputCrate>,
}

impl CopyKey {
    #[instrument(level = "trace", skip(below))]
    pub(super) fn new(
        krate: InputCrate,
        (enabled, disabled): &(Vec<String>, Vec<String>),
        below: Vec<InputCrate>,
    ) -> Self {
        Self {
            krate,
            enabled: enabled.clone(),
            disabled: disabled.clone(),
            below,
        }
    }
}

/// A crate's files as a context has them: each that was copied with its copy, and those that were not.
pub(super) type CrateFiles = (Vec<(FileId, FileId)>, Vec<FileId>);

/// A copy a context made, and the files of its crate as that context has them.
#[derive(Debug, Clone)]
pub(super) struct SharedCopy {
    pub(super) copy: InputCrate,
    /// The crate's files that the context copied, each with its copy.
    pub(super) files: Vec<(FileId, FileId)>,
    /// The crate's files that the context takes as they are.
    pub(super) aliased: Vec<FileId>,
}

/// What a context takes of the copies earlier contexts made.
#[derive(Debug, Default, Clone)]
pub(super) struct Borrowed {
    /// The copies of crates that are another context's.
    pub(super) crates: BTreeSet<InputCrate>,
    /// The files of those crates that the context that made them copied.
    pub(super) files: HashMap<FileId, FileId>,
    /// The files of those crates that it did not.
    pub(super) aliased: HashSet<FileId>,
}

impl Borrowed {
    /// Take `shared` too.
    pub(super) fn add(&mut self, shared: &SharedCopy) {
        self.crates.insert(shared.copy);
        self.files.extend(shared.files.iter().copied());
        self.aliased.extend(shared.aliased.iter().copied());
    }
}

/// The directory each local crate's files are under: the one its root file is in, or `None` when the
/// root file's place is not known.
#[instrument(level = "trace", skip_all)]
pub(super) fn directories_of(
    locals: &[(InputCrate, FileId)],
    files: &[(FileId, PathBuf)],
) -> Vec<(InputCrate, Option<PathBuf>)> {
    let path_of: HashMap<FileId, &PathBuf> =
        files.iter().map(|(file, path)| (*file, path)).collect();
    locals
        .iter()
        .map(|(krate, root)| {
            (
                *krate,
                path_of
                    .get(root)
                    .and_then(|path| path.parent())
                    .map(PathBuf::from),
            )
        })
        .collect()
}

/// Whether rust-analyzer reads an atom without a predicate in the text naming it: `#[test]` is active
/// where `test` is, a `debug_assert!` where `debug_assertions` is, and what the target is
/// decides which of a dependency's items exist. Every crate is taken to name these.
fn is_implicit(atom: &str) -> bool {
    let name = atom.split('=').next().unwrap_or(atom).trim();
    matches!(
        name,
        "test"
            | "debug_assertions"
            | "doc"
            | "doctest"
            | "miri"
            | "unix"
            | "windows"
            | "proc_macro"
            | "overflow_checks"
            | "panic"
    ) || name.starts_with("target_")
}

/// The part of `context`'s change that a crate naming `atoms` can tell: all of it for a crate whose
/// names are not all known (`None`).
#[instrument(level = "trace", skip(context, atoms))]
pub(super) fn restricted(
    context: &SemanticContextKey,
    atoms: Option<&BTreeSet<String>>,
) -> (Vec<String>, Vec<String>) {
    let keep =
        |text: &&String| atoms.is_none_or(|atoms| atoms.contains(*text)) || is_implicit(text);
    (
        context.enabled().iter().filter(keep).cloned().collect(),
        context.disabled().iter().filter(keep).cloned().collect(),
    )
}

impl SourceDatabase {
    /// Whether contexts share the copies they have in common, as the options say.
    pub(super) fn shares_copies(&self) -> bool {
        !self.copies_every_crate()
            && !self
                .cargo_options
                .as_ref()
                .is_some_and(|options| *options.do_not_share_context_copies())
    }

    /// The atoms the files of each crate name, or `None` for a crate that has a file that reads the
    /// cfg otherwise or whose gates are not known.
    #[instrument(level = "debug", skip_all)]
    pub(super) fn crate_atoms(
        &self,
        directories: &[(InputCrate, Option<PathBuf>)],
        files: &[(FileId, PathBuf)],
    ) -> HashMap<InputCrate, Option<BTreeSet<String>>> {
        let mut atoms: HashMap<InputCrate, Option<BTreeSet<String>>> = directories
            .iter()
            .map(|(krate, directory)| (*krate, directory.as_ref().map(|_| BTreeSet::new())))
            .collect();
        for (file, path) in files {
            for (krate, directory) in directories {
                if !directory
                    .as_ref()
                    .is_some_and(|directory| path.starts_with(directory))
                {
                    continue;
                }
                match atoms_of(&self.db, *file) {
                    Some(named) => {
                        if let Some(Some(all)) = atoms.get_mut(krate) {
                            all.extend(named.iter().cloned());
                        }
                    }
                    None => {
                        atoms.insert(*krate, None);
                    }
                }
            }
        }
        atoms
    }

    /// Give up the copies kept for the contexts to come: retire what they own, and forget them.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn release_shared_copies(&mut self, after: &str) {
        let held: Vec<InputCrate> = self
            .shared_copies
            .values()
            .map(|shared| shared.copy)
            .collect();
        self.shared_copies.clear();
        if !held.is_empty() {
            self.retire_crates(&held, after);
        }
    }
}
