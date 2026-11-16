//! A cfg context as crates of its own in the session database.
//!
//! The context pass used to change every local crate's cfg input and put it back,
//! and each change made every result Salsa had memoised stale. rust-analyzer
//! already supports the same root file in more than one crate (it keys such
//! crates by their data and cfg), and every query it answers is keyed by crate.
//! So a context is added as a copy of each local crate with its cfg changed, and
//! the ordinary crates are never touched: what they computed stays valid, and
//! the context's results coexist with theirs.

use std::collections::{BTreeSet, HashMap};
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
use super::contexts::LocalCfgs;
use crate::{CordialError, CordialResult, FunctionLocation, SemanticContextKey};

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
        Some(SavedContext {
            context: self.context.clone(),
            crates,
            files,
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
        Some(Self {
            context: saved.context,
            crates,
            files,
            kept: true,
        })
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
        self.files.get(&original).copied()
    }

    /// The original of each copy.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn originals(&self) -> HashMap<FileId, FileId> {
        self.files
            .iter()
            .map(|(original, copy)| (*copy, *original))
            .collect()
    }
}

impl SourceDatabase {
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
        match position {
            Some(position) if !self.context_crates[position].kept => {
                Ok(self.context_crates[position].clone())
            }
            Some(position) => {
                let previous = self.context_crates[position].clone();
                let added = self.build_context(context, position, Some(&previous))?;
                self.context_crates[position] = added.clone();
                Ok(added)
            }
            None => {
                let added = self.build_context(context, self.context_crates.len(), None)?;
                self.context_crates.push(added.clone());
                Ok(added)
            }
        }
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
        let ordinary = LocalCfgs::capture_excluding(&self.db, &self.shadow_crates());
        let files = self.copy_local_files(index, previous.map(|kept| &kept.files))?;
        let mut shadows: Vec<(InputCrate, InputCrate)> = Vec::new();
        for (krate, options) in ordinary.cfgs() {
            let mut changed = options.clone();
            changed.apply_diff(LocalCfgs::diff_of(context));
            let mut data = krate.data(&self.db).clone();
            // The copy is rooted at the copy of the root file.
            if let Some(root) = files.get(&data.root_file_id) {
                data.root_file_id = *root;
            }
            // A dependency on another local crate follows to that crate's copy,
            // so a test binary built for the context sees the library built for it.
            for dependency in &mut data.dependencies {
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
            let kept = previous.and_then(|kept| {
                kept.crates
                    .iter()
                    .find(|(original, _)| original == krate)
                    .map(|(_, copy)| *copy)
            });
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
                copy
            } else {
                InputCrate::builder(data, extra, workspace, changed, env)
                    .durability(Durability::LOW)
                    .new(&self.db)
            };
            shadows.push((*krate, copy));
        }
        self.register_shadows(index, &shadows);
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

    /// Copy every file of the workspace's own source roots to virtual paths under
    /// a source root of its own, returning each original with its copy.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn copy_local_files(
        &mut self,
        index: usize,
        known: Option<&HashMap<FileId, FileId>>,
    ) -> CordialResult<HashMap<FileId, FileId>> {
        let root_id = SourceRootId(FIRST_COPY_ROOT + u32::try_from(index).unwrap_or(u32::MAX >> 2));
        let roots: BTreeSet<SourceRootId> = attach_db(&self.db, || {
            Crate::all(&self.db)
                .into_iter()
                .filter(|krate| krate.origin(&self.db).is_local())
                .filter(|krate| !self.shadow_crates().contains(&krate.base()))
                .map(|krate| {
                    self.db
                        .file_source_root(krate.root_file(&self.db))
                        .source_root_id(&self.db)
                })
                .collect()
        });
        let mut set = FileSet::default();
        let mut copies: HashMap<FileId, FileId> = HashMap::new();
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
                let text = self.db.file_text(original).text(&self.db).to_string();
                self.vfs
                    .set_file_contents(copy_path.clone(), Some(text.clone().into_bytes()));
                let (copy, _) = self
                    .vfs
                    .file_id(&copy_path)
                    .ok_or_else(|| CordialError::not_found(real.as_str().into()))?;
                // A copy the database already holds is written only if it differs.
                if known.and_then(|known| known.get(&original)) == Some(&copy) {
                    if &**self.db.file_text(copy).text(&self.db) != text.as_str() {
                        self.db.set_file_text(copy, &text);
                    }
                    if self.db.file_source_root(copy).source_root_id(&self.db) != root_id {
                        self.db.set_file_source_root_with_durability(
                            copy,
                            root_id,
                            Durability::LOW,
                        );
                    }
                } else {
                    self.db.set_file_text(copy, &text);
                    self.db
                        .set_file_source_root_with_durability(copy, root_id, Durability::LOW);
                }
                set.insert(copy, copy_path);
                copies.insert(original, copy);
            }
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
        Ok(copies)
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
        for copy in copies {
            self.db.set_file_text(copy, text);
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
