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
    Crate as InputCrate, SourceDatabase as RaSourceDatabase, SourceRoot, SourceRootId, all_crates,
    set_all_crates_with_durability,
};
use ra_ap_vfs::{FileId, VfsPath, file_set::FileSet};
use salsa::Durability;
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

/// The first id given to the source roots that hold a context's file copies, far
/// above any id the workspace loader used.
const FIRST_COPY_ROOT: u32 = 0x4000_0000;

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
    /// Each declaration in the copy with the ordinary build's declaration at the
    /// same place in the source, so facts about the copy can speak of the original.
    #[getter(skip)]
    counterparts: HashMap<ModuleDef, ModuleDef>,
}

impl ContextCrates {
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

    /// The ordinary build's declaration of what `definition` declares in the copy.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn counterpart(&self, definition: ModuleDef) -> Option<ModuleDef> {
        self.counterparts.get(&definition).copied()
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
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    pub fn add_context_crates(
        &mut self,
        context: &SemanticContextKey,
    ) -> CordialResult<ContextCrates> {
        if let Some(existing) = self
            .context_crates
            .iter()
            .find(|added| added.context() == context)
        {
            return Ok(existing.clone());
        }
        let ordinary = LocalCfgs::capture_excluding(&self.db, &self.shadow_crates());
        let files = self.copy_local_files(self.context_crates.len())?;
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
            let copy = InputCrate::builder(
                data,
                krate.extra_data(&self.db).clone(),
                krate.workspace_data(&self.db).clone(),
                changed,
                krate.env(&self.db).clone(),
            )
            .durability(Durability::MEDIUM)
            .new(&self.db);
            shadows.push((*krate, copy));
        }
        let mut registered: Vec<InputCrate> = all_crates(&self.db).iter().copied().collect();
        registered.extend(shadows.iter().map(|(_, copy)| *copy));
        set_all_crates_with_durability(&mut self.db, registered, Durability::MEDIUM);
        let counterparts = self.counterparts_of(&shadows, &files);
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
        let added = ContextCrates {
            context: context.clone(),
            crates: shadows,
            files,
            counterparts,
        };
        self.context_crates.push(added.clone());
        Ok(added)
    }

    /// Copy every file of the workspace's own source roots to virtual paths under
    /// a source root of its own, returning each original with its copy.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn copy_local_files(&mut self, index: usize) -> CordialResult<HashMap<FileId, FileId>> {
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
                self.db.set_file_text(copy, &text);
                self.db
                    .set_file_source_root_with_durability(copy, root_id, Durability::LOW);
                set.insert(copy, copy_path);
                copies.insert(original, copy);
            }
        }
        self.db.set_source_root_with_durability(
            root_id,
            Arc::new(SourceRoot::new_local(set)),
            Durability::LOW,
        );
        Ok(copies)
    }

    /// Pair each declaration in a copy with the ordinary build's at the same place
    /// in the source: the same file, the same name token, the same kind of item.
    #[instrument(level = "debug", skip(self, pairs, files))]
    #[hotpath::measure(impl_type = "SourceDatabase")]
    fn counterparts_of(
        &self,
        pairs: &[(InputCrate, InputCrate)],
        files: &HashMap<FileId, FileId>,
    ) -> HashMap<ModuleDef, ModuleDef> {
        let originals: HashMap<FileId, FileId> = files
            .iter()
            .map(|(original, copy)| (*copy, *original))
            .collect();
        attach_db(&self.db, || {
            let mut found = HashMap::new();
            for (ordinary, copy) in pairs {
                let at: HashMap<_, _> = declared_in(&self.db, Crate::from(*ordinary))
                    .into_iter()
                    .filter_map(|definition| {
                        Some((place_of(&self.db, definition, &HashMap::new())?, definition))
                    })
                    .collect();
                for definition in declared_in(&self.db, Crate::from(*copy)) {
                    if let Some(place) = place_of(&self.db, definition, &originals)
                        && let Some(ordinary) = at.get(&place)
                    {
                        found.insert(definition, *ordinary);
                    }
                }
            }
            found
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

/// The kind of item and the original place its name is written: the key two
/// declarations share when one is the copy of the other.
type Place = super::item_key::DefSite;

/// Every declaration a crate makes that facts can name: its functions, types,
/// traits and constants, including the associated items of impls and traits.
#[instrument(level = "debug", skip(db))]
#[hotpath::measure]
fn declared_in(db: &RootDatabase, krate: Crate) -> Vec<ModuleDef> {
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

/// Where `definition`'s name is written in the original source, with `originals`
/// naming a copy's file by its original's.
#[instrument(level = "trace", skip(db, definition, originals))]
fn place_of(
    db: &RootDatabase,
    definition: ModuleDef,
    originals: &HashMap<FileId, FileId>,
) -> Option<Place> {
    let mut site = super::item_key::site_of(db, definition)?;
    site.file = originals.get(&site.file).copied().unwrap_or(site.file);
    Some(site)
}
