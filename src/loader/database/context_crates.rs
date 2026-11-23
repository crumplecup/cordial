//! A cfg context as crates of its own in the session database.
//!
//! The context pass used to change every local crate's cfg input and put it back,
//! and each change made every result Salsa had memoised stale. rust-analyzer
//! already supports the same root file in more than one crate (it keys such
//! crates by their data and cfg), and every query it answers is keyed by crate.
//! So a context is added as a copy of each local crate with its cfg changed, and
//! the ordinary crates are never touched: what they computed stays valid, and
//! the context's results coexist with theirs.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use triomphe::Arc;

use ra_ap_hir::{AssocItem, Crate, Function, HasSource, ModuleDef, attach_db};
use ra_ap_ide_db::RootDatabase;
use ra_ap_ide_db::base_db::{
    Crate as InputCrate, SourceDatabase as RaSourceDatabase, SourceRoot, SourceRootId,
    set_tool_root_crates,
};
use ra_ap_vfs::{FileId, VfsPath, file_set::FileSet};
use salsa::Durability;
use salsa::Setter as _;
use tracing::instrument;

use super::SourceDatabase;
use super::context_shares::{
    Borrowed, CopyKey, CrateFiles, Owners, SharedCopy, directories_of, restricted,
};
use super::contexts::{Classified, Leaves};
use super::contexts::{LocalCfgs, atom};
use crate::{CordialError, CordialResult, FunctionLocation, SemanticContextKey};
use ra_ap_cfg::CfgDiff;

/// The start of the virtual path a context's copy of a file lives at; the context's
/// number and the file's real path follow it.
const COPY_PATH_PREFIX: &str = "/__cordial_context_";

/// The real path a context's virtual copy was made from, or `None` for a path that
/// is not a copy.
#[instrument(level = "trace", skip(path))]
pub(super) fn original_path(path: &VfsPath) -> Option<std::path::PathBuf> {
    let text = path.to_string();
    let rest = text.strip_prefix(COPY_PATH_PREFIX)?;
    let (_, real) = rest.split_once('/')?;
    Some(std::path::PathBuf::from(format!("/{real}")))
}

/// The source root of the context whose copy a path is, or `None` for a path that is not a
/// copy: a context's files are all in one source root of their own.
#[instrument(level = "trace", skip(path))]
pub(super) fn copy_root_of(path: &VfsPath) -> Option<SourceRootId> {
    let text = path.to_string();
    let rest = text.strip_prefix(COPY_PATH_PREFIX)?;
    let (index, _) = rest.split_once('/')?;
    Some(SourceRootId(FIRST_COPY_ROOT + index.parse::<u32>().ok()?))
}

/// The first id given to the source roots that hold a context's file copies, far
/// above any id the workspace loader used.
const FIRST_COPY_ROOT: u32 = ra_ap_ide_db::base_db::TOOL_SOURCE_ROOTS;

// The crates and copies a context adds are written during a run, after results exist that
// were saved: a write of the watermark durability or higher makes every restored result stale
// (see `salsa::flatten`), so they are the workspace's own (LOW), like the files they copy.

/// A context's copy of the workspace: the same source files under a source root of
/// its own, and a copy of each local crate rooted at them under the context's cfg.
///
/// rust-analyzer finds a file's crates through its source root, and takes the
/// first. A file in two crates would always resolve in the first, so each copy has
/// files of its own: every query for it is keyed by them, nothing the ordinary build
/// computed is touched, and the dependency crates are shared.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct ContextCrates {
    context: SemanticContextKey,
    #[getter(skip)]
    crates: Vec<(InputCrate, InputCrate)>,
    /// Each original file with its copy.
    #[getter(skip)]
    files: HashMap<FileId, FileId>,
    /// The original files this context takes as they are: its source root names them, under the
    /// path a copy would have had, and nothing is copied.
    #[getter(skip)]
    aliased: std::collections::HashSet<FileId>,
    /// The copies this context takes from earlier contexts, with their files.
    #[getter(skip)]
    borrowed: Borrowed,
    /// Whether this came from a saved state and has not been brought up to date with the
    /// sources of this run since: its copies and crates are the ones the last run made.
    #[getter(skip)]
    kept: bool,
}

/// What of a context is saved with the database: which crates and files are its, by where
/// they are in the saved database. The copies are inputs of the database already; this
/// is the way back to them.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct SavedContext {
    context: SemanticContextKey,
    /// Each original crate with its copy, as positions in `all_crates`.
    crates: Vec<(usize, usize)>,
    files: Vec<(FileId, FileId)>,
    /// The crates that are another context's copies, as positions in `all_crates`.
    #[serde(default)]
    borrowed_crates: Vec<usize>,
    /// Their files that were copied, each with its copy.
    #[serde(default)]
    borrowed_files: Vec<(FileId, FileId)>,
    /// Their files that were not.
    #[serde(default)]
    borrowed_aliased: Vec<FileId>,
}

impl ContextCrates {
    /// What to save of this context, or `None` when a crate of it is not registered.
    #[instrument(level = "trace", skip(self, order))]
    pub(super) fn to_saved(&self, order: &[InputCrate]) -> Option<SavedContext> {
        let position = |wanted: &InputCrate| order.iter().position(|krate| krate == wanted);
        let crates = self
            .crates
            .iter()
            .map(|(original, copy)| Some((position(original)?, position(copy)?)))
            .collect::<Option<Vec<_>>>()?;
        let mut files: Vec<(FileId, FileId)> = self.files.iter().map(|(a, b)| (*a, *b)).collect();
        files.sort_by_key(|(original, _)| original.index());
        let borrowed_crates = self
            .borrowed
            .crates
            .iter()
            .map(position)
            .collect::<Option<Vec<_>>>()?;
        let mut borrowed_files: Vec<(FileId, FileId)> =
            self.borrowed.files.iter().map(|(a, b)| (*a, *b)).collect();
        borrowed_files.sort_by_key(|(original, _)| original.index());
        let mut borrowed_aliased: Vec<FileId> = self.borrowed.aliased.iter().copied().collect();
        borrowed_aliased.sort_by_key(|file| file.index());
        Some(SavedContext {
            context: self.context.clone(),
            crates,
            files,
            borrowed_crates,
            borrowed_files,
            borrowed_aliased,
        })
    }

    /// The context a saved one was, found again in the restored database whose
    /// `all_crates` is `order`. It is as the last run left it until it is asked for.
    #[instrument(level = "trace", skip(saved, order))]
    pub(super) fn from_saved(saved: SavedContext, order: &[InputCrate]) -> Option<Self> {
        let crates = saved
            .crates
            .into_iter()
            .map(|(original, copy)| Some((*order.get(original)?, *order.get(copy)?)))
            .collect::<Option<Vec<_>>>()?;
        let files: HashMap<FileId, FileId> = saved.files.into_iter().collect();
        let borrowed = Borrowed {
            crates: saved
                .borrowed_crates
                .into_iter()
                .map(|position| order.get(position).copied())
                .collect::<Option<_>>()?,
            files: saved.borrowed_files.into_iter().collect(),
            aliased: saved.borrowed_aliased.into_iter().collect(),
        };
        Some(Self {
            context: saved.context,
            crates,
            files,
            aliased: std::collections::HashSet::new(),
            borrowed,
            kept: true,
        })
    }

    /// How many crates this context copied: each is a definition map to build under its cfg.
    #[instrument(level = "trace", skip(self))]
    pub fn crate_count(&self) -> usize {
        self.crates.len()
    }

    /// The crates this context added, as a set.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn crate_set(&self) -> BTreeSet<InputCrate> {
        self.shadows().collect()
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn shadows(&self) -> impl Iterator<Item = InputCrate> + '_ {
        self.crates.iter().map(|(_, shadow)| *shadow)
    }

    /// This context's copy of `original`, if it has one.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn copy_of(&self, original: FileId) -> Option<FileId> {
        self.files
            .get(&original)
            .or_else(|| self.borrowed.files.get(&original))
            .copied()
    }

    /// Whether this context takes `original` as it is, and not as a copy.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn is_aliased(&self, original: FileId) -> bool {
        self.aliased.contains(&original) && !self.borrowed.files.contains_key(&original)
    }

    /// The original of each copy.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn originals(&self) -> HashMap<FileId, FileId> {
        self.files
            .iter()
            .chain(self.borrowed.files.iter())
            .map(|(original, copy)| (*copy, *original))
            .collect()
    }
}

/// What copying a context's files decided: each file's copy, the files taken as they are, and the crates
/// that are copied.
struct CopiedFiles {
    copies: HashMap<FileId, FileId>,
    aliased: std::collections::HashSet<FileId>,
    crates: BTreeSet<InputCrate>,
    /// For each copied crate, the part of the context's change that it can tell.
    changes: HashMap<InputCrate, (Vec<String>, Vec<String>)>,
    /// The copies of crates that an earlier context made and this one takes, by the original crate.
    shared: HashMap<InputCrate, SharedCopy>,
    /// For each crate this context copies, its files and what was done with them: the copy of each
    /// that was copied, and the ones that were not.
    homes: HashMap<InputCrate, CrateFiles>,
    /// The dependencies a crate follows to the context's copy of them: (crate, dependency).
    follows: HashSet<(InputCrate, InputCrate)>,
    /// For each crate this context copies itself, the files of the original it cannot take as they are.
    affected_files: HashMap<InputCrate, Vec<FileId>>,
}

impl SourceDatabase {
    /// Whether every local crate is copied for a context, as its options say, and not only the ones the
    /// context changes. For the tests of what the pass does.
    /// Whether the options turn off working out which modules a copy takes from the original.
    fn shares_no_definition_maps(&self) -> bool {
        self.cargo_options
            .as_ref()
            .is_some_and(|options| *options.do_not_share_definition_maps())
    }

    pub(super) fn copies_every_crate(&self) -> bool {
        self.cargo_options
            .as_ref()
            .is_some_and(|options| *options.copy_every_crate())
    }

    /// The crates a context copies of the local crates `locals` (each with its root file), given the
    /// workspace's `files` with their paths and the `changed` ones.
    ///
    /// A crate is changed when a file under its source directory is. Its copy is needed, and so is the copy
    /// of each crate it depends on that itself depends, through crates, on a changed one: what that crate
    /// defines can differ in the context through the changed crate, so the copy of what depends on it has
    /// to see its copy. A crate that depends on none, and is not itself changed, defines the same in the
    /// context as it does without it, and the copies use the ordinary one.
    ///
    /// A crate whose root file is not known is taken to be changed.
    #[instrument(level = "debug", skip(self, locals, files, changed))]
    fn crates_to_copy(
        &self,
        locals: &[(InputCrate, FileId)],
        files: &[(FileId, PathBuf)],
        changed: &BTreeSet<FileId>,
        follows: &HashSet<(InputCrate, InputCrate)>,
    ) -> BTreeSet<InputCrate> {
        let path_of: HashMap<FileId, &PathBuf> =
            files.iter().map(|(file, path)| (*file, path)).collect();
        let directories = directories_of(locals, files);
        let mut changed_crates: BTreeSet<InputCrate> = directories.unplaced().collect();
        for file in changed {
            let Some(path) = path_of.get(file) else {
                continue;
            };
            changed_crates.extend(directories.owners_of(path));
        }
        // The local dependencies of each local crate.
        let dependencies = self.local_dependencies(locals);
        // Whether a crate is changed or follows, through crates it follows, to one that is.
        let mut needs: HashMap<InputCrate, bool> = HashMap::new();
        fn needs_copy(
            krate: InputCrate,
            changed: &BTreeSet<InputCrate>,
            dependencies: &HashMap<InputCrate, Vec<InputCrate>>,
            follows: &HashSet<(InputCrate, InputCrate)>,
            memo: &mut HashMap<InputCrate, bool>,
        ) -> bool {
            if let Some(known) = memo.get(&krate) {
                return *known;
            }
            // A cycle cannot be among a crate's dependencies; if it were, no crate in it reaches
            // the changed one by it.
            memo.insert(krate, false);
            let answer = changed.contains(&krate)
                || dependencies.get(&krate).is_some_and(|below| {
                    below.iter().any(|dependency| {
                        follows.contains(&(krate, *dependency))
                            && needs_copy(*dependency, changed, dependencies, follows, memo)
                    })
                });
            memo.insert(krate, answer);
            answer
        }
        // The changed crates, and what a copied crate follows that is changed or follows one that is.
        let mut copied = changed_crates.clone();
        let mut pending: Vec<InputCrate> = changed_crates.iter().copied().collect();
        while let Some(krate) = pending.pop() {
            for dependency in dependencies.get(&krate).into_iter().flatten() {
                if follows.contains(&(krate, *dependency))
                    && needs_copy(
                        *dependency,
                        &changed_crates,
                        &dependencies,
                        follows,
                        &mut needs,
                    )
                    && copied.insert(*dependency)
                {
                    pending.push(*dependency);
                }
            }
        }
        copied
    }

    /// The names that a flip in each crate, or in what it depends on, makes visible to a crate that depends
    /// on it (see [`Self::follows_of`]).
    fn visible_names(
        &self,
        locals: &[(InputCrate, FileId)],
        owners: &Owners,
        files: &[(FileId, PathBuf)],
        pairs: &[(ra_ap_cfg::CfgOptions, ra_ap_cfg::CfgOptions)],
        classified: &Classified,
        dependencies: &HashMap<InputCrate, Vec<InputCrate>>,
    ) -> HashMap<InputCrate, BTreeSet<String>> {
        use super::contexts::{Reach, flipped_names_of};
        // The names the gates the context flips in each crate make visible.
        let mut flipped: HashMap<InputCrate, BTreeSet<String>> = HashMap::new();
        for (file, path) in files {
            if classified.why.get(file) != Some(&"flipped") {
                continue;
            }
            let names = flipped_names_of(&self.db, *file, pairs, Reach::Defined);
            for krate in owners.owners_of(path) {
                flipped
                    .entry(krate)
                    .or_default()
                    .extend(names.iter().cloned());
            }
        }
        // What may differ for a crate that depends on `krate`: its own and what it depends on's.
        fn visible(
            krate: InputCrate,
            flipped: &HashMap<InputCrate, BTreeSet<String>>,
            dependencies: &HashMap<InputCrate, Vec<InputCrate>>,
            memo: &mut HashMap<InputCrate, BTreeSet<String>>,
        ) -> BTreeSet<String> {
            if let Some(known) = memo.get(&krate) {
                return known.clone();
            }
            memo.insert(krate, BTreeSet::new());
            let mut names = flipped.get(&krate).cloned().unwrap_or_default();
            for dependency in dependencies.get(&krate).into_iter().flatten() {
                names.extend(visible(*dependency, flipped, dependencies, memo));
            }
            memo.insert(krate, names.clone());
            names
        }
        let mut memo: HashMap<InputCrate, BTreeSet<String>> = HashMap::new();
        locals
            .iter()
            .map(|(krate, _)| (*krate, visible(*krate, &flipped, dependencies, &mut memo)))
            .collect()
    }

    /// For each crate this context copies, the files of the original that the copy cannot take as they are:
    /// the files the context copies, and the files it takes from the ordinary build that nevertheless mention
    /// a name a gate it flips defines, in the crate or in what it depends on. The copy's module closure starts
    /// from these (see `hir_def::nameres::shared`).
    fn affected_files_of(
        &self,
        owners: &Owners,
        files: &[(FileId, PathBuf)],
        copied_files: &BTreeSet<FileId>,
        crates: &BTreeSet<InputCrate>,
        visible_through: &HashMap<InputCrate, BTreeSet<String>>,
    ) -> HashMap<InputCrate, Vec<FileId>> {
        let mut affected: HashMap<InputCrate, Vec<FileId>> = HashMap::new();
        for (file, path) in files {
            for krate in owners
                .owners_of(path)
                .filter(|krate| crates.contains(krate))
            {
                let sensitive = !copied_files.contains(file)
                    && visible_through.get(&krate).is_some_and(|names| {
                        !names.is_empty() && {
                            let text = self.db.file_text(*file).text(&self.db).clone();
                            // A glob import brings in whatever the module it names has: nothing yet, under the
                            // ordinary cfg, if what the context switches is all it has.
                            text.contains("::*")
                                || text.contains("use *")
                                || text
                                    .split(|character: char| {
                                        !(character.is_alphanumeric() || character == '_')
                                    })
                                    .any(|word| names.contains(word))
                        }
                    });
                if copied_files.contains(file) || sensitive {
                    affected.entry(krate).or_default().push(*file);
                }
            }
        }
        for files in affected.values_mut() {
            files.sort_by_key(|file| file.index());
        }
        affected
    }

    /// The dependencies each crate follows to the context's copy of them, as (crate, dependency): those
    /// where the crate mentions a name that a gate the context flips makes visible, in the dependency or in
    /// what the dependency depends on. A crate that mentions none sees the dependency as the ordinary build
    /// has it, whatever the context switched there, so it is not made to depend on a copy of it: that is
    /// what lets crates that differ in nothing they use share a copy across contexts.
    #[instrument(level = "debug", skip_all)]
    fn follows_of(
        &self,
        owners: &Owners,
        files: &[(FileId, PathBuf)],
        dependencies: &HashMap<InputCrate, Vec<InputCrate>>,
        visible_through: &HashMap<InputCrate, BTreeSet<String>>,
    ) -> HashSet<(InputCrate, InputCrate)> {
        // The words each crate's files hold, for the crates that depend on something that can differ.
        let mut words: HashMap<InputCrate, HashSet<String>> = HashMap::new();
        for (krate, below) in dependencies {
            if below.iter().all(|dependency| {
                visible_through
                    .get(dependency)
                    .is_none_or(BTreeSet::is_empty)
            }) {
                continue;
            }
            let mut held: HashSet<String> = HashSet::new();
            for (file, path) in files {
                if !owners.crate_owns(*krate, path) {
                    continue;
                }
                let text = self.db.file_text(*file).text(&self.db).clone();
                held.extend(
                    text.split(|character: char| {
                        !(character.is_alphanumeric() || character == '_')
                    })
                    .filter(|word| !word.is_empty())
                    .map(str::to_owned),
                );
            }
            words.insert(*krate, held);
        }
        let mut follows = HashSet::new();
        for (krate, below) in dependencies {
            // A crate whose files are not known may use anything.
            let unplaced = owners.unplaced().any(|unplaced| unplaced == *krate);
            for dependency in below {
                let Some(names) = visible_through
                    .get(dependency)
                    .filter(|names| !names.is_empty())
                else {
                    continue;
                };
                let uses = unplaced
                    || words
                        .get(krate)
                        .is_some_and(|held| names.iter().any(|name| held.contains(name)));
                if uses {
                    follows.insert((*krate, *dependency));
                }
            }
        }
        follows
    }

    /// The local dependencies of each of `locals`.
    fn local_dependencies(
        &self,
        locals: &[(InputCrate, FileId)],
    ) -> HashMap<InputCrate, Vec<InputCrate>> {
        let analysed: BTreeSet<InputCrate> = locals.iter().map(|(krate, _)| *krate).collect();
        attach_db(&self.db, || {
            locals
                .iter()
                .map(|(krate, _)| {
                    let local: Vec<InputCrate> = krate
                        .data(&self.db)
                        .dependencies
                        .iter()
                        .map(|dependency| dependency.crate_id)
                        .filter(|dependency| analysed.contains(dependency))
                        .collect();
                    (*krate, local)
                })
                .collect()
        })
    }

    /// Add `context` as a copy of the workspace beside the ordinary one, once:
    /// asking again for a context already added returns it. The ordinary crates'
    /// and files' inputs are not written, so nothing computed for them is
    /// invalidated.
    ///
    /// A context the last run saved is taken up again, not made again: its crates and
    /// files are the ones in the restored database, and only what differs from the
    /// sources of this run is written, so what the database memoised for them stays.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn add_context_crates(
        &mut self,
        context: &SemanticContextKey,
    ) -> CordialResult<ContextCrates> {
        let position = self
            .context_crates
            .iter()
            .position(|added| added.context() == context);
        let added = match position {
            Some(position) if !self.context_crates[position].kept => {
                self.context_crates[position].clone()
            }
            Some(position) => {
                let previous = self.context_crates[position].clone();
                let added = self.build_context(context, position, Some(&previous))?;
                self.context_crates[position] = added.clone();
                added
            }
            None => {
                let added = self.build_context(context, self.context_crates.len(), None)?;
                self.context_crates.push(added.clone());
                added
            }
        };
        let roots: Vec<_> = added
            .shadows()
            .map(|krate| salsa::plumbing::AsId::as_id(&krate))
            .collect();
        salsa::activate_owner_roots(&mut self.db, &roots);
        Ok(added)
    }

    /// Make context number `index`, or bring `previous` (the same context, as the last
    /// run left it) up to date.
    #[instrument(level = "debug", skip(self, previous), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn build_context(
        &mut self,
        context: &SemanticContextKey,
        index: usize,
        previous: Option<&ContextCrates>,
    ) -> CordialResult<ContextCrates> {
        let ordinary = LocalCfgs::capture(&self.db, &self.analysed_crates());
        let pairs = ordinary.pairs(context);
        let CopiedFiles {
            copies: files,
            aliased,
            crates: copied,
            changes,
            shared,
            mut homes,
            follows,
            affected_files,
        } = self.copy_local_files(index, previous.map(|kept| &kept.files), &pairs, context)?;
        let mut shadows: Vec<(InputCrate, InputCrate)> = Vec::new();
        let mut borrowed = Borrowed::default();
        for (krate, options) in ordinary.cfgs() {
            // A crate the context does not change is the ordinary crate: what depends on it follows
            // to it, and what was built for it is shared.
            if !copied.contains(krate) {
                continue;
            }
            // A copy an earlier context made of the same crate under the same change is taken as it is.
            if let Some(taken) = shared.get(krate) {
                borrowed.add(taken);
                shadows.push((*krate, taken.copy));
                continue;
            }
            let mut changed = options.clone();
            changed.apply_diff(match changes.get(krate) {
                Some((enabled, disabled)) => CfgDiff::new(
                    enabled.iter().map(|text| atom(text)).collect(),
                    disabled.iter().map(|text| atom(text)).collect(),
                ),
                None => LocalCfgs::diff_of(context),
            });
            let mut data = krate.data(&self.db).clone();
            // The copy is rooted at the copy of the root file.
            if let Some(root) = files.get(&data.root_file_id) {
                data.root_file_id = *root;
            }
            // A dependency on another local crate follows to that crate's copy,
            // so a test binary built for the context sees the library built for it.
            for dependency in &mut data.dependencies {
                if !follows.contains(&(*krate, dependency.crate_id)) {
                    continue;
                }
                if let Some((_, copy)) = shadows
                    .iter()
                    .find(|(original, _)| *original == dependency.crate_id)
                {
                    dependency.crate_id = *copy;
                }
            }
            let extra = krate.extra_data(&self.db).clone();
            let workspace = krate.workspace_data(&self.db).clone();
            let env = krate.env(&self.db).clone();
            let affected = affected_files.get(krate).cloned().unwrap_or_default();
            let kept = previous.and_then(|kept| {
                kept.crates
                    .iter()
                    .find(|(original, copy)| {
                        original == krate && !kept.borrowed.crates.contains(copy)
                    })
                    .map(|(_, copy)| *copy)
            });
            let below: Vec<InputCrate> = data
                .dependencies
                .iter()
                .map(|dependency| dependency.crate_id)
                .collect();
            let copy = if let Some(copy) = kept {
                // A crate a restored database holds is written only where it differs
                // from this run's sources: Salsa does not compare an input to its old
                // value, so writing an equal one would invalidate what read it.
                macro_rules! write_if_different {
                    ($field:ident, $set:ident, $value:expr) => {{
                        let value = $value;
                        if *copy.$field(&self.db) != value {
                            copy.$set(&mut self.db)
                                .with_durability(Durability::LOW)
                                .to(value);
                        }
                    }};
                }
                write_if_different!(data, set_data, data);
                write_if_different!(extra_data, set_extra_data, extra);
                write_if_different!(workspace_data, set_workspace_data, workspace);
                write_if_different!(cfg_options, set_cfg_options, changed);
                write_if_different!(env, set_env, env);
                write_if_different!(canonical, set_canonical, Some(*krate));
                write_if_different!(affected_files, set_affected_files, affected);
                copy
            } else {
                InputCrate::builder(data, extra, workspace, changed, env)
                    .canonical(Some(*krate))
                    .affected_files(affected)
                    .durability(Durability::LOW)
                    .new(&self.db)
            };
            shadows.push((*krate, copy));
            if self.shares_copies() && !self.shares_no_definition_maps() {
                // Which of the copy's modules it takes from the original (see `hir_def::nameres::shared`).
                let _ = ra_ap_hir_def::nameres::shared::shared_modules(&self.db, copy);
            }
            if self.shares_copies()
                && let Some(change) = changes.get(krate)
            {
                let (copies, aliased) = homes.remove(krate).unwrap_or_default();
                self.shared_copies.insert(
                    CopyKey::new(*krate, change, below),
                    SharedCopy {
                        copy,
                        files: copies,
                        aliased,
                    },
                );
            }
        }
        let own: Vec<(InputCrate, InputCrate)> = shadows
            .iter()
            .filter(|(_, copy)| !borrowed.crates.contains(copy))
            .copied()
            .collect();
        self.register_shadows(index, &own);
        let bytes: usize = files
            .values()
            .map(|copy| self.db.file_text(*copy).text(&self.db).len())
            .sum();
        crate::storage::record_context_copy(
            &context.to_string(),
            files.len(),
            bytes,
            shadows.len(),
        );
        Ok(ContextCrates {
            context: context.clone(),
            crates: shadows,
            files,
            aliased,
            borrowed,
            kept: false,
        })
    }

    /// Register `shadows` as the crates of context number `index`, in the source root its
    /// copies are in. Nothing is written when they are the ones already there, and no other
    /// context's registration is touched: what was computed for another context does not
    /// depend on this one.
    #[instrument(level = "debug", skip(self, shadows))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn register_shadows(&mut self, index: usize, shadows: &[(InputCrate, InputCrate)]) {
        let root = SourceRootId(FIRST_COPY_ROOT + u32::try_from(index).unwrap_or(u32::MAX >> 2));
        let crates: std::sync::Arc<[InputCrate]> = shadows.iter().map(|(_, copy)| *copy).collect();
        set_tool_root_crates(&mut self.db, root, crates, Durability::LOW);
    }

    /// Copy the files of the workspace's own source roots that the context can change to virtual
    /// paths under a source root of its own, and name the others there as they are (see
    /// [`reuse`](super::contexts)). Returns each original with its copy, and the originals taken as
    /// they are.
    #[instrument(
        level = "debug",
        skip(self, known, pairs, context),
        err(level = "warn")
    )]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn copy_local_files(
        &mut self,
        index: usize,
        known: Option<&HashMap<FileId, FileId>>,
        pairs: &[(ra_ap_cfg::CfgOptions, ra_ap_cfg::CfgOptions)],
        context: &SemanticContextKey,
    ) -> CordialResult<CopiedFiles> {
        let root_id = SourceRootId(FIRST_COPY_ROOT + u32::try_from(index).unwrap_or(u32::MAX >> 2));
        let analysed = self.analysed_crates();
        let (roots, locals): (BTreeSet<SourceRootId>, Vec<(InputCrate, FileId)>) =
            attach_db(&self.db, || {
                let locals: Vec<_> = Crate::all(&self.db)
                    .into_iter()
                    .filter(|krate| analysed.contains(&krate.base()))
                    .collect();
                let roots = locals
                    .iter()
                    .map(|krate| {
                        self.db
                            .file_source_root(krate.root_file(&self.db))
                            .source_root_id(&self.db)
                    })
                    .collect();
                let locals = locals
                    .iter()
                    .map(|krate| (krate.base(), krate.root_file(&self.db)))
                    .collect();
                (roots, locals)
            });
        // Every file of the workspace, with where it is.
        let mut originals: Vec<(FileId, VfsPath, PathBuf)> = Vec::new();
        for root in roots {
            let source_root = self.db.source_root(root).source_root(&self.db).clone();
            for original in source_root.iter() {
                let Some(path) = source_root.path_for_file(&original) else {
                    continue;
                };
                let Some(real) = path.as_path() else { continue };
                let copy_path = VfsPath::new_virtual_path(format!(
                    "{COPY_PATH_PREFIX}{index}{}",
                    real.as_str()
                ));
                originals.push((original, copy_path, PathBuf::from(real.as_str())));
            }
        }
        // Only Rust files are decided: the others (a manifest, a data file a macro includes) are the
        // original's, found where the original is.
        let located: Vec<(FileId, PathBuf)> = originals
            .iter()
            .filter(|(_, _, path)| path.extension().is_some_and(|extension| extension == "rs"))
            .map(|(file, _, path)| (*file, path.clone()))
            .collect();
        let classified = self.classify_files(
            &located,
            pairs,
            if !context.enabled().iter().any(|atom| atom == "test") {
                Leaves::Never
            } else if context.enabled().len() == 1 && context.disabled().is_empty() {
                Leaves::All
            } else {
                Leaves::Mentioning
            },
            &locals.iter().map(|(_, root)| *root).collect::<Vec<_>>(),
        );
        let directories = directories_of(&locals, &located);
        // Which dependency each crate follows to the context's copy of it: only where it can tell the
        // difference, by mentioning a name a flip there (or in what it depends on) makes visible.
        let dependencies = self.local_dependencies(&locals);
        let visible = self.visible_names(
            &locals,
            &directories,
            &located,
            pairs,
            &classified,
            &dependencies,
        );
        let follows = if self.copies_every_crate() {
            self.local_dependencies(&locals)
                .into_iter()
                .flat_map(|(krate, below)| {
                    below.into_iter().map(move |dependency| (krate, dependency))
                })
                .collect()
        } else {
            self.follows_of(&directories, &located, &dependencies, &visible)
        };
        // The crates this context copies: the ones it changes, and the dependencies on the way to
        // them. A crate it does not change is the ordinary crate, whose maps are built and shared.
        let copied = if self.copies_every_crate() {
            locals.iter().map(|(krate, _)| *krate).collect()
        } else {
            self.crates_to_copy(&locals, &located, &classified.changed, &follows)
        };
        // Why crates are changed: for each reason a file is changed, how many crates have such a file
        // under their directory, how many crates each such file is under, and which files say so.
        {
            let path_of: HashMap<FileId, &PathBuf> =
                located.iter().map(|(file, path)| (*file, path)).collect();
            let mut crates_by: std::collections::BTreeMap<&str, BTreeSet<InputCrate>> =
                Default::default();
            let mut files_by: std::collections::BTreeMap<&str, (u64, u64)> = Default::default();
            let mut examples: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
            for (file, reason) in &classified.why {
                let Some(path) = path_of.get(file) else {
                    continue;
                };
                let owners: Vec<InputCrate> = directories.owners_of(path).collect();
                let entry = files_by.entry(reason).or_default();
                entry.0 += 1;
                entry.1 += owners.len() as u64;
                crates_by.entry(reason).or_default().extend(owners);
                let shown = examples.entry(reason).or_default();
                if shown.len() < 3 {
                    shown.push(path.display().to_string());
                }
            }
            let summary: Vec<String> = files_by
                .iter()
                .map(|(reason, (files, owned))| {
                    format!(
                        "{reason}: {files} files in {} crates ({owned} file-crate pairs), e.g. {:?}",
                        crates_by.get(reason).map_or(0, BTreeSet::len),
                        examples.get(reason).cloned().unwrap_or_default()
                    )
                })
                .collect();
            tracing::info!(
                target: "cordial::semantic::contexts",
                context_index = index,
                why = %summary.join(" | "),
                "why this context's files are changed, and how many crates each reaches"
            );
        }
        // What of the context's change each copied crate can tell, and which of the copies are ones an
        // earlier context made of the same crate under the same change.
        let sharing = self.shares_copies();
        let atoms = if sharing {
            self.crate_atoms(&directories, &located)
        } else {
            HashMap::new()
        };
        let mut changes: HashMap<InputCrate, (Vec<String>, Vec<String>)> = HashMap::new();
        for (krate, _) in &locals {
            if sharing && copied.contains(krate) {
                changes.insert(
                    *krate,
                    restricted(context, atoms.get(krate).and_then(Option::as_ref)),
                );
            }
        }
        let mut shared: HashMap<InputCrate, SharedCopy> = HashMap::new();
        if sharing {
            // The copy each crate has in this context: `None` while it is a copy this context makes.
            let mut effective: HashMap<InputCrate, Option<InputCrate>> = HashMap::new();
            attach_db(&self.db, || {
                for (krate, _) in &locals {
                    let Some(change) = changes.get(krate) else {
                        continue;
                    };
                    let mut below = Vec::new();
                    let mut own_below = false;
                    for dependency in &krate.data(&self.db).dependencies {
                        if !follows.contains(&(*krate, dependency.crate_id)) {
                            below.push(dependency.crate_id);
                            continue;
                        }
                        match effective.get(&dependency.crate_id) {
                            Some(Some(copy)) => below.push(*copy),
                            Some(None) => own_below = true,
                            None => below.push(dependency.crate_id),
                        }
                    }
                    let taken = (!own_below)
                        .then(|| self.shared_copies.get(&CopyKey::new(*krate, change, below)))
                        .flatten();
                    match taken {
                        Some(taken) => {
                            effective.insert(*krate, Some(taken.copy));
                            shared.insert(*krate, taken.clone());
                        }
                        None => {
                            effective.insert(*krate, None);
                        }
                    }
                }
            });
        }
        // What this context copies itself: the crates it takes from earlier contexts have their files
        // there.
        let (classified, crate_roots) = if shared.is_empty() {
            let roots = locals
                .iter()
                .filter(|(krate, _)| copied.contains(krate))
                .map(|(_, root)| *root)
                .collect();
            (classified, roots)
        } else {
            let path_of: HashMap<FileId, &PathBuf> =
                located.iter().map(|(file, path)| (*file, path)).collect();
            let Classified {
                changed,
                reuse,
                why,
            } = classified;
            let changed = changed
                .into_iter()
                .filter(|file| {
                    path_of.get(file).is_none_or(|path| {
                        directories
                            .owners_of(path)
                            .any(|krate| copied.contains(&krate) && !shared.contains_key(&krate))
                    })
                })
                .collect();
            let roots = locals
                .iter()
                .filter(|(krate, _)| copied.contains(krate) && !shared.contains_key(krate))
                .map(|(_, root)| *root)
                .collect();
            (
                Classified {
                    changed,
                    reuse,
                    why,
                },
                roots,
            )
        };
        let copying = classified.copying(&located, &crate_roots);
        tracing::info!(
            target: "cordial::semantic::contexts",
            context_index = index,
            crates = locals.len(),
            crates_copied = copied.len(),
            crates_shared = shared.len(),
            files = copying.reuse.files,
            flipped = copying.reuse.flipped,
            unsure = copying.reuse.unsure,
            spine = copying.reuse.spine,
            aliased = copying.reuse.aliased(),
            "the files this context copies, and the files it takes from the ordinary build as they are"
        );
        let mut set = FileSet::default();
        let mut copies: HashMap<FileId, FileId> = HashMap::new();
        let mut aliased: std::collections::HashSet<FileId> = std::collections::HashSet::new();
        for (original, copy_path, _) in originals {
            if !copying.copy.contains(&original) {
                // The context's root names the original itself under the path a copy would have.
                set.insert(original, copy_path);
                aliased.insert(original);
                continue;
            }
            // The copy holds the allocation the original's text is in, not a copy of it.
            let text = self.db.file_text(original).text(&self.db).clone();
            self.vfs
                .set_file_contents(copy_path.clone(), Some(text.as_bytes().to_vec()));
            let (copy, _) = self
                .vfs
                .file_id(&copy_path)
                .ok_or_else(|| CordialError::not_found(copy_path.to_string().into()))?;
            // A copy the database already holds is written only if it differs.
            if known.and_then(|known| known.get(&original)) == Some(&copy) {
                if *self.db.file_text(copy).text(&self.db) != *text {
                    self.db.set_file_text_arc(copy, text.clone());
                }
                if self.db.file_source_root(copy).source_root_id(&self.db) != root_id {
                    self.db
                        .set_file_source_root_with_durability(copy, root_id, Durability::LOW);
                }
            } else {
                self.db.set_file_text_arc(copy, text.clone());
                self.db
                    .set_file_source_root_with_durability(copy, root_id, Durability::LOW);
            }
            set.insert(copy, copy_path);
            copies.insert(original, copy);
        }
        let unchanged = known.is_some() && {
            let current = self.db.source_root(root_id).source_root(&self.db).clone();
            current.iter().collect::<BTreeSet<_>>() == set.iter().collect::<BTreeSet<_>>()
        };
        if !unchanged {
            self.db.set_source_root_with_durability(
                root_id,
                Arc::new(SourceRoot::new_local(set)),
                Durability::LOW,
            );
        }
        let text_shared = copies
            .iter()
            .filter(|(original, copy)| {
                triomphe::Arc::ptr_eq(
                    &self.db.file_text(**original).text(&self.db),
                    &self.db.file_text(**copy).text(&self.db),
                )
            })
            .count();
        tracing::debug!(
            target: "cordial::context_crates",
            copies = copies.len(),
            text_shared,
            "the files copied for a context, and how many of them hold the original's text itself"
        );
        // Each crate's files as this context has them, for the contexts that take its copy.
        let mut homes: HashMap<InputCrate, CrateFiles> = HashMap::new();
        if sharing {
            for (krate, placed) in directories.crates() {
                if !placed || !copied.contains(&krate) || shared.contains_key(&krate) {
                    continue;
                }
                let entry = homes.entry(krate).or_default();
                for (file, path) in &located {
                    if !directories.crate_owns(krate, path) {
                        continue;
                    }
                    match copies.get(file) {
                        Some(copy) => entry.0.push((*file, *copy)),
                        None => entry.1.push(*file),
                    }
                }
            }
        }
        let own: BTreeSet<InputCrate> = copied
            .iter()
            .filter(|krate| !shared.contains_key(krate))
            .copied()
            .collect();
        let affected_files = if sharing {
            self.affected_files_of(&directories, &located, &copying.copy, &own, &visible)
        } else {
            HashMap::new()
        };
        Ok(CopiedFiles {
            copies,
            aliased,
            crates: copied,
            changes,
            shared,
            homes,
            follows,
            affected_files,
        })
    }

    /// Bring every context's copy of `original` up to date with `text`.
    #[instrument(level = "debug", skip(self, text))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn change_context_copies(&mut self, original: FileId, text: &str) {
        let copies: Vec<FileId> = self
            .context_crates
            .iter()
            .filter_map(|added| added.copy_of(original))
            .collect();
        // The original has the new text by now: its copies take its allocation. Were it another
        // (a text not applied to it), they share one of their own.
        let original_text = self.db.file_text(original).text(&self.db).clone();
        let shared = if *original_text == *text {
            original_text
        } else {
            triomphe::Arc::from(text)
        };
        for copy in copies {
            self.db.set_file_text_arc(copy, shared.clone());
        }
    }

    /// The functions declared in the ordinary build's local crates.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn ordinary_functions(&self) -> BTreeSet<FunctionLocation> {
        let shadows = self.shadow_crates();
        self.locations(|krate| !shadows.contains(&krate.base()), &HashMap::new())
    }

    /// The functions declared in the crates `context` added: those active under
    /// its cfg, at the locations they have in the original sources.
    #[instrument(level = "debug", skip(self, context))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn context_functions(&self, context: &ContextCrates) -> BTreeSet<FunctionLocation> {
        let mine: BTreeSet<InputCrate> = context.shadows().collect();
        self.locations(|krate| mine.contains(&krate.base()), &context.originals())
    }

    /// The workspace's own crates as the ordinary build has them: the local
    /// crates, without the copies contexts added.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn ordinary_crates(&self) -> BTreeSet<InputCrate> {
        let shadows = self.shadow_crates();
        attach_db(&self.db, || {
            Crate::all(&self.db)
                .into_iter()
                .filter(|krate| krate.origin(&self.db).is_local())
                .map(|krate| krate.base())
                .filter(|krate| !shadows.contains(krate))
                .collect()
        })
    }

    /// The crates cfg contexts are about: the ordinary build's local crates that belong to a
    /// workspace member. A path dependency is local to rust-analyzer (it has no registry source)
    /// but nothing of it is analysed, so a gate in its files asks for no context and a context
    /// does not copy it: each copy is a definition map to build under the context's cfg, and the
    /// copies of a project's dependencies are the larger share of a context's cost.
    ///
    /// A crate belongs to the package whose directory is the deepest one holding its root file.
    /// With no workspace, or a crate in no package, every local crate counts.
    #[instrument(level = "debug", skip(self))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub(super) fn analysed_crates(&self) -> BTreeSet<InputCrate> {
        let local = self.ordinary_crates();
        let Some(workspace) = self.workspace.as_ref() else {
            return local;
        };
        let packages: Vec<(&std::path::Path, bool)> = workspace
            .packages()
            .iter()
            .map(|package| (package.root().as_path(), true))
            .chain(
                self.external_packages
                    .iter()
                    .map(|package| (package.root().as_path(), false)),
            )
            .collect();
        attach_db(&self.db, || {
            local
                .into_iter()
                .filter(|base| {
                    let krate = Crate::from(*base);
                    let Some(root) =
                        super::item_key::vfs_path_of(&self.db, krate.root_file(&self.db))
                    else {
                        return true;
                    };
                    let Some(root) = root.as_path() else {
                        return true;
                    };
                    let root: &std::path::Path = root.as_ref();
                    packages
                        .iter()
                        .filter(|(directory, _)| root.starts_with(directory))
                        .max_by_key(|(directory, _)| directory.components().count())
                        .is_none_or(|(_, member)| *member)
                })
                .collect()
        })
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn shadow_crates(&self) -> BTreeSet<InputCrate> {
        self.context_crates
            .iter()
            .flat_map(ContextCrates::shadows)
            .collect()
    }

    /// Locations of the functions in the local crates `keep` accepts, with a
    /// copy's file named by its original's.
    #[instrument(level = "debug", skip(self, keep, originals))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn locations(
        &self,
        keep: impl Fn(&Crate) -> bool,
        originals: &HashMap<FileId, FileId>,
    ) -> BTreeSet<FunctionLocation> {
        attach_db(&self.db, || {
            let mut found = BTreeSet::new();
            for krate in Crate::all(&self.db) {
                if !krate.origin(&self.db).is_local() || !keep(&krate) {
                    continue;
                }
                for function in functions_of(&self.db, krate) {
                    let source = function.source(&self.db);
                    let Some(source) = source else { continue };
                    if source.file_id.is_macro() {
                        continue;
                    }
                    let copy = source.file_id.original_file(&self.db).file_id(&self.db);
                    let file = originals.get(&copy).copied().unwrap_or(copy);
                    let Some(path) = self.vfs.file_path(file).as_path() else {
                        continue;
                    };
                    let start = u32::from(
                        ra_ap_syntax::AstNode::syntax(&source.value)
                            .text_range()
                            .start(),
                    );
                    found.insert(FunctionLocation::new(
                        AsRef::<std::path::Path>::as_ref(path).to_path_buf(),
                        start,
                    ));
                }
            }
            found
        })
    }
}

/// Every function a crate declares: free functions, trait methods and the
/// methods of each impl.
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
fn functions_of(db: &RootDatabase, krate: Crate) -> Vec<Function> {
    let mut functions = Vec::new();
    for module in krate.modules(db) {
        for declaration in module.declarations(db) {
            match declaration {
                ModuleDef::Function(function) => functions.push(function),
                ModuleDef::Trait(declared) => functions.extend(
                    declared
                        .items(db)
                        .into_iter()
                        .filter_map(|item| match item {
                            AssocItem::Function(function) => Some(function),
                            _ => None,
                        }),
                ),
                _ => {}
            }
        }
        for implementation in module.impl_defs(db) {
            functions.extend(
                implementation
                    .items(db)
                    .into_iter()
                    .filter_map(|item| match item {
                        AssocItem::Function(function) => Some(function),
                        _ => None,
                    }),
            );
        }
    }
    functions
}

/// Every declaration a crate makes that facts can name: its functions, types,
/// traits and constants, including the associated items of impls and traits.
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
pub(super) fn declared_in(db: &RootDatabase, krate: Crate) -> Vec<ModuleDef> {
    let mut found = Vec::new();
    for module in krate.modules(db) {
        found.extend(module.declarations(db));
        for implementation in module.impl_defs(db) {
            found.extend(implementation.items(db).into_iter().map(associated));
        }
    }
    let traits: Vec<_> = found
        .iter()
        .filter_map(|definition| match definition {
            ModuleDef::Trait(declared) => Some(*declared),
            _ => None,
        })
        .collect();
    for declared in traits {
        found.extend(declared.items(db).into_iter().map(associated));
    }
    found
}

fn associated(item: AssocItem) -> ModuleDef {
    match item {
        AssocItem::Function(function) => ModuleDef::Function(function),
        AssocItem::Const(constant) => ModuleDef::Const(constant),
        AssocItem::TypeAlias(alias) => ModuleDef::TypeAlias(alias),
    }
}
