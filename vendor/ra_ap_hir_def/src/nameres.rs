//! This module implements import-resolution/macro expansion algorithm.
//!
//! The result of this module is `DefMap`: a data structure which contains:
//!
//!   * a tree of modules for the crate
//!   * for each module, a set of items visible in the module (directly declared
//!     or imported)
//!
//! Note that `DefMap` contains fully macro expanded code.
//!
//! Computing `DefMap` can be partitioned into several logically
//! independent "phases". The phases are mutually recursive though, there's no
//! strict ordering.
//!
//! ## Collecting RawItems
//!
//! This happens in the `raw` module, which parses a single source file into a
//! set of top-level items. Nested imports are desugared to flat imports in this
//! phase. Macro calls are represented as a triple of `(Path, Option<Name>,
//! TokenTree)`.
//!
//! ## Collecting Modules
//!
//! This happens in the `collector` module. In this phase, we recursively walk
//! tree of modules, collect raw items from submodules, populate module scopes
//! with defined items (so, we assign item ids in this phase) and record the set
//! of unresolved imports and macros.
//!
//! While we walk tree of modules, we also record macro_rules definitions and
//! expand calls to macro_rules defined macros.
//!
//! ## Resolving Imports
//!
//! We maintain a list of currently unresolved imports. On every iteration, we
//! try to resolve some imports from this list. If the import is resolved, we
//! record it, by adding an item to current module scope and, if necessary, by
//! recursively populating glob imports.
//!
//! ## Resolving Macros
//!
//! macro_rules from the same crate use a global mutable namespace. We expand
//! them immediately, when we collect modules.
//!
//! Macros from other crates (including proc-macros) can be used with
//! `foo::bar!` syntax. We handle them similarly to imports. There's a list of
//! unexpanded macros. On every iteration, we try to resolve each macro call
//! path and, upon success, we run macro expansion and "collect module" phase on
//! the result

pub mod assoc;
pub mod attr_resolution;
mod collector;
pub mod diagnostics;
mod mod_resolution;
mod path_resolution;
pub mod proc_macro;
pub mod shared;
pub mod stable;

#[cfg(test)]
mod tests;

use std::ops::{Deref, DerefMut, Index, IndexMut};

use base_db::{Crate, SourceDatabase};
use either::Either;
use hir_expand::{
    EditionedFileId, ErasedAstId, HirFileId, InFile, MacroCallId, mod_path::ModPath, name::Name,
    proc_macro::ProcMacroKind,
};
use intern::{Symbol, sym};
use itertools::Itertools;
use rustc_hash::FxHashMap;
use salsa::plumbing::{AsId, FromId};
use span::{Edition, FileAstId, FileId, ROOT_ERASED_FILE_AST_ID};
use stdx::format_to;
use syntax::{AstNode, SmolStr, SyntaxNode, ToSmolStr, ast};
use triomphe::Arc;
use tt::TextRange;

use crate::{
    AstId, BlockId, BlockIdLt, BuiltinDeriveImplId, ExternCrateId, FunctionId, FxIndexMap, Lookup,
    MacroCallStyles, MacroExpander, MacroId, ModuleId, ModuleIdLt, ProcMacroId, UseId,
    item_scope::{BuiltinShadowMode, ItemScope},
    item_tree::{FileItemTreeKey, TreeId},
    nameres::{diagnostics::DefDiagnostic, path_resolution::ResolveMode},
    per_ns::PerNs,
    unstable_features::UnstableFeatures,
    visibility::{Visibility, VisibilityExplicitness},
};

pub use self::path_resolution::ResolvePathResultPrefixInfo;

#[cfg(test)]
thread_local! {
    /// HACK: In order to test builtin derive expansion, we gate their fast path with this atomic when cfg(test).
    pub(crate) static ENABLE_BUILTIN_DERIVE_FAST_PATH: std::cell::Cell<bool> =
        const { std::cell::Cell::new(true) };
}

#[inline]
#[cfg(test)]
fn enable_builtin_derive_fast_path() -> bool {
    ENABLE_BUILTIN_DERIVE_FAST_PATH.get()
}

#[inline(always)]
#[cfg(not(test))]
fn enable_builtin_derive_fast_path() -> bool {
    true
}

const PREDEFINED_TOOLS: &[SmolStr] = &[
    SmolStr::new_static("clippy"),
    SmolStr::new_static("rustfmt"),
    SmolStr::new_static("diagnostic"),
    SmolStr::new_static("miri"),
    SmolStr::new_static("rust_analyzer"),
];

/// Parts of the def map that are only needed when analyzing code in the same crate.
///
/// There are some data in the def map (e.g. extern prelude) that is only needed when analyzing
/// things in the same crate (and maybe in the IDE layer), e.g. the extern prelude. If we put
/// it in the DefMap dependant DefMaps will be invalidated when they change (e.g. when we add
/// a dependency to the crate). Instead we split them out of the DefMap into a LocalDefMap struct.
/// `crate_local_def_map()` returns both, and `crate_def_map()` returns only the external-relevant
/// DefMap.
#[derive(Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct LocalDefMap {
    // FIXME: There are probably some other things that could be here, but this is less severe and you
    // need to be careful with things that block def maps also have.
    /// The extern prelude which contains all root modules of external crates that are in scope.
    extern_prelude: FxIndexMap<Name, (ModuleId, Option<ExternCrateId>)>,
}

impl std::hash::Hash for LocalDefMap {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let LocalDefMap { extern_prelude } = self;
        extern_prelude.len().hash(state);
        for (name, (crate_root, extern_crate)) in extern_prelude {
            name.hash(state);
            crate_root.hash(state);
            extern_crate.hash(state);
        }
    }
}

impl LocalDefMap {
    pub(crate) const EMPTY: &Self =
        &Self { extern_prelude: FxIndexMap::with_hasher(rustc_hash::FxBuildHasher) };

    fn shrink_to_fit(&mut self) {
        let Self { extern_prelude } = self;
        extern_prelude.shrink_to_fit();
    }

    pub(crate) fn extern_prelude(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&Name, (ModuleId, Option<ExternCrateId>))> + '_ {
        self.extern_prelude.iter().map(|(name, &def)| (name, def))
    }
}

/// Contains the results of (early) name resolution.
///
/// A `DefMap` stores the module tree and the definitions that are in scope in every module after
/// item-level macros have been expanded.
///
/// Every crate has a primary `DefMap` whose root is the crate's main file (`main.rs`/`lib.rs`),
/// computed by the `crate_def_map` query. Additionally, every block expression introduces the
/// opportunity to write arbitrary item and module hierarchies, and thus gets its own `DefMap` that
/// is computed by the `block_def_map` query.
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DefMap {
    /// The crate this `DefMap` belongs to.
    krate: Crate,
    /// When this is a block def map, this will hold the block id of the block and module that
    /// contains this block.
    block: Option<BlockInfo>,
    pub root: ModuleId,
    /// The modules and their data declared in this crate.
    modules: ModulesMap,
    /// The modules of a finished map, a cell each: empty while the collector builds, and then `modules` is.
    cells: FxIndexMap<ModuleId, ModuleCell>,
    /// The prelude module for this crate. This either comes from an import
    /// marked with the `prelude_import` attribute, or (in the normal case) from
    /// a dependency (`std` or `core`).
    /// The prelude is empty for non-block DefMaps (unless `#[prelude_import]` was used,
    /// but that attribute is nightly and when used in a block, it affects resolution globally
    /// so we aren't handling this correctly anyways).
    prelude: Option<(ModuleId, Option<UseId>)>,
    /// `macro_use` prelude that contains macros from `#[macro_use]`'d external crates. Note that
    /// this contains all kinds of macro, not just `macro_rules!` macro.
    /// ExternCrateId being None implies it being imported from the general prelude import.
    macro_use_prelude: FxHashMap<Name, (MacroId, Option<ExternCrateId>)>,

    /// Tracks which custom derives are in scope for an item, to allow resolution of derive helper
    /// attributes.
    // FIXME: Figure out a better way for the IDE layer to resolve these?
    derive_helpers_in_scope:
        FxHashMap<AstId<ast::Item>, Vec<(Name, MacroId, Either<MacroCallId, BuiltinDeriveImplId>)>>,
    /// A mapping from [`hir_expand::MacroDefId`] to [`crate::MacroId`].
    pub macro_def_to_macro_id: FxHashMap<ErasedAstId, MacroId>,

    /// The diagnostics that need to be emitted for this crate, while the collector builds; then `diagnostics_cell` has them.
    diagnostics: Vec<DefDiagnostic>,
    diagnostics_cell: Option<DiagnosticsCell>,

    /// The crate data that is shared between a crate's def map and all its block def maps.
    data: Arc<DefMapCrateData>,
}

/// Data that belongs to a crate which is shared between a crate's def map and all its block def maps.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct DefMapCrateData {
    /// Side table for resolving derive helpers.
    exported_derives: FxHashMap<MacroId, Box<[Name]>>,
    fn_proc_macro_mapping: FxHashMap<FunctionId, ProcMacroId>,
    fn_proc_macro_mapping_back: FxHashMap<ProcMacroId, FunctionId>,

    /// Custom tool modules registered with `#![register_tool]`.
    registered_tools: Vec<Symbol>,
    /// Unstable features of Rust enabled with `#![feature(A, B)]`.
    unstable_features: UnstableFeatures,
    /// `#[rustc_coherence_is_core]`
    rustc_coherence_is_core: bool,
    no_core: bool,
    no_std: bool,

    edition: Edition,
    recursion_limit: Option<u32>,
}

impl DefMapCrateData {
    fn new(edition: Edition) -> Self {
        Self {
            exported_derives: FxHashMap::default(),
            fn_proc_macro_mapping: FxHashMap::default(),
            fn_proc_macro_mapping_back: FxHashMap::default(),
            registered_tools: PREDEFINED_TOOLS.iter().map(|it| Symbol::intern(it)).collect(),
            unstable_features: UnstableFeatures::default(),
            rustc_coherence_is_core: false,
            no_core: false,
            no_std: false,
            edition,
            recursion_limit: None,
        }
    }

    fn shrink_to_fit(&mut self) {
        let Self {
            exported_derives,
            fn_proc_macro_mapping,
            fn_proc_macro_mapping_back,
            registered_tools,
            unstable_features,
            rustc_coherence_is_core: _,
            no_core: _,
            no_std: _,
            edition: _,
            recursion_limit: _,
        } = self;
        exported_derives.shrink_to_fit();
        fn_proc_macro_mapping.shrink_to_fit();
        fn_proc_macro_mapping_back.shrink_to_fit();
        registered_tools.shrink_to_fit();
        unstable_features.shrink_to_fit();
    }
}

/// For `DefMap`s computed for a block expression, this stores its location in the parent map.
#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize, serde::Deserialize)]
struct BlockInfo {
    /// The `BlockId` this `DefMap` was created from.
    block: BlockId,
    /// The containing module.
    parent: ModuleId,
}

/// A crate said by its name and its number in both bases, so that a line of the log names it: a
/// bare `Crate(Id(3c1f))` is a number to look up elsewhere, in a base the tables do not print.
fn crate_label(krate: Crate) -> String {
    salsa::with_attached_database(|db| {
        use salsa::plumbing::AsId;
        let bits = krate.as_id().as_bits();
        let name = krate
            .extra_data(db)
            .display_name
            .as_ref()
            .map_or_else(|| "(unnamed)".to_owned(), ToString::to_string);
        format!("{name} (crate {bits} = {bits:#x})")
    })
    .unwrap_or_else(|| "(no database attached)".to_owned())
}

impl DefMap {
    /// A module's data. Reading a finished map's module depends on that module's cell and on no other.
    pub fn module_data<'a>(&'a self, db: &'a dyn SourceDatabase, id: ModuleIdLt<'_>) -> &'a ModuleData {
        let id = unsafe { id.to_static() };
        self.try_module_data(db, id).unwrap_or_else(|| {
            // Said as an event first, so that the log has the steps that were running, which the
            // panic's message does not.
            let module_crate = salsa::with_attached_database(|db| id.krate(db))
                .map_or_else(|| "(no database attached)".to_owned(), crate_label);
            tracing::error!(
                target: "cordial::nameres",
                def_map_crate = %crate_label(self.krate()),
                module_crate = %module_crate,
                def_map_root = ?self.root,
                module = ?id,
                "a module is not in the def map it was looked up in"
            );
            panic!("ModuleId not found in ModulesMap {:#?}: {id:#?}", self.root)
        })
    }

    /// A module's data if the map has the module.
    pub fn try_module_data<'a>(
        &'a self,
        db: &'a dyn SourceDatabase,
        id: ModuleIdLt<'_>,
    ) -> Option<&'a ModuleData> {
        let id = unsafe { id.to_static() };
        if self.cells.is_empty() {
            self.modules.get(&id)
        } else {
            self.cells.get(&id).map(|cell| cell.data(db))
        }
    }

    /// Makes the finished map's modules cells, so that what reads one depends on that one.
    pub(crate) fn freeze(&mut self, db: &dyn SourceDatabase) {
        let diagnostics = std::mem::take(&mut self.diagnostics);
        let cell = DiagnosticsCellLt::new(db, self.krate, diagnostics);
        self.diagnostics_cell = Some(unsafe { cell.to_static() });
        let modules = std::mem::replace(&mut self.modules, ModulesMap::new());
        for (id, data) in modules.inner {
            let cell = ModuleCellLt::new(db, id, data);
            self.cells.insert(id, unsafe { cell.to_static() });
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, serde::Serialize, serde::Deserialize)]
pub enum ModuleOrigin {
    CrateRoot {
        definition: EditionedFileId,
    },
    /// Note that non-inline modules, by definition, live inside non-macro file.
    File {
        is_mod_rs: bool,
        declaration: FileAstId<ast::Module>,
        declaration_tree_id: TreeId,
        definition: EditionedFileId,
    },
    Inline {
        definition_tree_id: TreeId,
        definition: FileAstId<ast::Module>,
    },
    /// Pseudo-module introduced by a block scope (contains only inner items).
    BlockExpr {
        id: BlockId,
        block: AstId<ast::BlockExpr>,
    },
}

impl ModuleOrigin {
    pub fn declaration(&self) -> Option<AstId<ast::Module>> {
        match self {
            &ModuleOrigin::File { declaration, declaration_tree_id, .. } => {
                Some(AstId::new(declaration_tree_id.file_id(), declaration))
            }
            &ModuleOrigin::Inline { definition, definition_tree_id } => {
                Some(AstId::new(definition_tree_id.file_id(), definition))
            }
            ModuleOrigin::CrateRoot { .. } | ModuleOrigin::BlockExpr { .. } => None,
        }
    }

    pub fn file_id(&self) -> Option<EditionedFileId> {
        match self {
            ModuleOrigin::File { definition, .. } | ModuleOrigin::CrateRoot { definition } => {
                Some(*definition)
            }
            _ => None,
        }
    }

    pub fn is_inline(&self) -> bool {
        match self {
            ModuleOrigin::Inline { .. } | ModuleOrigin::BlockExpr { .. } => true,
            ModuleOrigin::CrateRoot { .. } | ModuleOrigin::File { .. } => false,
        }
    }

    /// Returns a node which defines this module.
    /// That is, a file or a `mod foo {}` with items.
    pub fn definition_source(&self, db: &dyn SourceDatabase) -> InFile<ModuleSource> {
        match self {
            &ModuleOrigin::File { definition: editioned_file_id, .. }
            | &ModuleOrigin::CrateRoot { definition: editioned_file_id } => {
                let sf = editioned_file_id.parse(db).tree();
                InFile::new(editioned_file_id.into(), ModuleSource::SourceFile(sf))
            }
            &ModuleOrigin::Inline { definition, definition_tree_id } => InFile::new(
                definition_tree_id.file_id(),
                ModuleSource::Module(
                    AstId::new(definition_tree_id.file_id(), definition).to_node(db),
                ),
            ),
            ModuleOrigin::BlockExpr { block, .. } => {
                InFile::new(block.file_id, ModuleSource::BlockExpr(block.to_node(db)))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModuleData {
    /// Where does this module come from?
    pub origin: ModuleOrigin,
    /// Declared visibility of this module.
    pub visibility: Visibility,
    /// Parent module in the same `DefMap`.
    ///
    /// [`None`] for block modules because they are always its `DefMap`'s root.
    pub parent: Option<ModuleId>,
    pub children: FxIndexMap<Name, ModuleId>,
    pub scope: ItemScope,
}

/// One module's data, as a tracked field: a reader of it depends on this module and not on the map.
///
/// The collector builds a map of plain [`ModuleData`]. When it has finished, the map is frozen into a cell per
/// module (`DefMap::freeze`). The cell is found again by the module's id when the query that made it runs
/// again, and its data keeps the revision it last changed in when equal, so a map in which only one module
/// differs is equal to the map before, and only what read that module's cell runs again.
#[salsa::tracked(debug, persist)]
pub struct ModuleCellLt<'db> {
    #[returns(copy)]
    pub module: ModuleIdLt<'db>,
    #[tracked]
    #[returns(ref)]
    pub data: ModuleData,
}
pub type ModuleCell = ModuleCellLt<'static>;

impl<'db> ModuleCellLt<'db> {
    /// # Safety
    ///
    /// The cell is kept in the map of the query that made it, and read through the database that made it.
    unsafe fn to_static(self) -> ModuleCell {
        unsafe { std::mem::transmute(self) }
    }
}

/// A finished map's diagnostics. They hold the crate's options, so they differ under any flip of the cfg: a
/// reader of a module does not depend on them, and a reader of them (the diagnostics of a module) does.
#[salsa::tracked(debug, persist)]
pub struct DiagnosticsCellLt<'db> {
    #[returns(copy)]
    pub krate: Crate,
    #[tracked]
    #[returns(ref)]
    pub diagnostics: Vec<DefDiagnostic>,
}
pub type DiagnosticsCell = DiagnosticsCellLt<'static>;

impl<'db> DiagnosticsCellLt<'db> {
    /// # Safety
    ///
    /// The cell is kept in the map of the query that made it, and read through the database that made it.
    unsafe fn to_static(self) -> DiagnosticsCell {
        unsafe { std::mem::transmute(self) }
    }
}

#[inline]
pub fn crate_def_map(db: &dyn SourceDatabase, crate_id: Crate) -> &DefMap {
    external_def_map(db, crate_id)
}

/// Local declaration and cached member keys, for retiring completed semantic values.
///
/// Read the persisted owner directly so restoring a context does not recreate the
/// external-map projection just to enumerate these keys. The owner may decode or
/// recompute on this read. Member/variant lists are read only when their existing memos
/// are verified and final in this revision, including encoded saved results.
/// Follow existing tracked-output links to already-current block maps and repeat
/// declaration selection there. Absent or stale member/block queries are not
/// executed just to retire their values. Missing producer records add no keys.
/// Local declarative expanders are selected from interned macro locations,
/// without compiling them; builtins, procedural and imported macros are excluded.
/// Item trees use explicit file/block-and-crate keys from the same local owners.
pub fn crate_declared_query_keys(
    db: &mut dyn SourceDatabase,
    crate_id: Crate,
) -> Vec<salsa::Id> {
    let mut keys = def_map_declared_query_keys(db, Either::Left(crate_id));
    let mut pending = keys.clone();
    pending.push(crate_id.as_id());
    let mut visited_blocks = rustc_hash::FxHashSet::default();
    loop {
        let outputs = db.tracked_output_keys(&pending);
        let blocks = db.cached_query_keys(&outputs, &["block_def_map"]);
        pending.clear();
        for key in blocks {
            if visited_blocks.insert(key) {
                // This query's exact input is a BlockId; the memo selector has
                // checked its ingredient and current/final value before this read.
                let block = BlockId::from_id(key);
                pending.extend(def_map_declared_query_keys(db, Either::Right(block)));
            }
        }
        if pending.is_empty() {
            break;
        }
        keys.extend(pending.iter().copied());
    }
    keys
}

fn def_map_declared_query_keys(
    db: &mut dyn SourceDatabase,
    owner: Either<Crate, BlockId>,
) -> Vec<salsa::Id> {
    let map: &DefMap = match owner {
        Either::Left(krate) => &crate_local_def_map(db, krate).def_map,
        Either::Right(block) => block_def_map(db, block),
    };
    let mut keys: Vec<_> = map.declared_query_keys(db).collect();
    keys.extend(map.item_tree_query_keys(db));
    let krate = map.krate;
    keys.extend(map.macro_def_to_macro_id.values().filter_map(|&mac| {
        mac.declarative_expander_key(db).map(|key| key.as_id())
    }));
    let owners: Vec<_> = map.own_modules(db).flat_map(|(_, module)| {
        module.scope.declarations()
            .filter_map(|def| match def {
                crate::ModuleDefId::TraitId(id) => Some(Either::Left(id)),
                _ => None,
            })
            .chain(module.scope.impls().map(Either::Right))
    }).collect();
    let enums: Vec<_> = map.own_modules(db)
        .flat_map(|(_, module)| module.scope.declarations())
        .filter_map(|def| match def {
            crate::ModuleDefId::AdtId(crate::AdtId::EnumId(id)) => Some(id),
            _ => None,
        })
        .collect();
    let mut owner_keys: Vec<_> = owners.iter()
        .map(|owner| match owner {
            Either::Left(id) => id.as_id(),
            Either::Right(id) => id.as_id(),
        })
        .collect();
    owner_keys.extend(enums.iter().map(|id| id.as_id()));
    if owner_keys.is_empty() {
        return keys;
    }
    let cached: rustc_hash::FxHashSet<_> = db
        .cached_query_keys(
            &owner_keys,
            &["TraitItems::query_with_diagnostics_", "ImplItems::of_", "EnumVariants::of_"],
        )
        .into_iter()
        .collect();
    tracing::debug!(
        target: "cordial::eviction",
        owners = owners.len() as u64,
        enums = enums.len() as u64,
        members_cached = cached.len() as u64,
        "the traits, impls and enums of a crate, and how many of them have their members cached"
    );
    for owner in owners {
        let (items, calls) = match owner {
            Either::Left(id) if cached.contains(&id.as_id()) => {
                let items = id.trait_items(db);
                (&items.items, &items.macro_calls)
            }
            Either::Right(id) if cached.contains(&id.as_id()) => {
                let items = id.impl_items(db);
                (&items.items, &items.macro_calls)
            }
            _ => continue,
        };
        keys.extend(items.iter().map(|(_, id)| id.as_id()));
        keys.extend(calls.iter().map(|(_, id)| id.as_id()));
        keys.extend(calls.iter().map(|(_, id)| {
            FileItemTreeKey::new(db, HirFileId::from(*id), krate).as_id()
        }));
    }
    for id in enums {
        if cached.contains(&id.as_id()) {
            keys.extend(id.enum_variants(db).variants.values().map(|(id, _)| id.as_id()));
        }
    }
    keys
}

// Keep the large payloads in the query memo so spilling that memo can release them.
// Tracked module/block identities are still created by the same owner query.
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct DefMapPair {
    pub(crate) def_map: Arc<DefMap>,
    pub(crate) local: Arc<LocalDefMap>,
}

// Independent projections backdate external and local changes separately. They share
// allocations with the persisted owner, and must also be evicted to release the maps.
#[salsa::tracked(returns(ref))]
fn external_def_map(db: &dyn SourceDatabase, crate_id: Crate) -> Arc<DefMap> {
    Arc::clone(&crate_local_def_map(db, crate_id).def_map)
}

#[salsa::tracked(returns(ref))]
pub(crate) fn local_def_map(db: &dyn SourceDatabase, crate_id: Crate) -> Arc<LocalDefMap> {
    Arc::clone(&crate_local_def_map(db, crate_id).local)
}

#[salsa::tracked(returns(ref), persist)]
pub(crate) fn crate_local_def_map(db: &dyn SourceDatabase, crate_id: Crate) -> DefMapPair {
    let krate = crate_id.data(db);
    let _p = tracing::info_span!(
        "crate_def_map_query",
        name=?crate_id
            .extra_data(db)
            .display_name
            .as_ref()
            .map(|it| it.crate_name().to_smolstr())
            .unwrap_or_default()
    )
    .entered();

    let root_file_id = crate_id.root_file_id(db);
    let module_data = ModuleData::new(
        ModuleOrigin::CrateRoot { definition: root_file_id },
        Visibility::Public,
        None,
    );

    let def_map = DefMap::empty(
        db,
        crate_id,
        Arc::new(DefMapCrateData::new(krate.edition)),
        module_data,
        None,
    );
    let (def_map, local_def_map) =
        collector::collect_defs(db, def_map, TreeId::new(root_file_id.into(), None), None);

    let mut def_map = def_map;
    def_map.freeze(db);
    DefMapPair { def_map: Arc::new(def_map), local: Arc::new(local_def_map) }
}

#[salsa::tracked(returns(ref), persist)]
pub fn block_def_map<'db>(db: &'db dyn SourceDatabase, block_id: BlockIdLt<'db>) -> DefMap {
    let _p = tracing::info_span!("block_def_map_query").entered();
    let block_id = unsafe { block_id.to_static() };
    let ast_id = block_id.ast_id(db);
    let module = unsafe { block_id.module(db).to_static() };

    let visibility = Visibility::Module(module, VisibilityExplicitness::Implicit);
    let module_data =
        ModuleData::new(ModuleOrigin::BlockExpr { block: ast_id, id: block_id }, visibility, None);

    let krate = module.krate(db);
    tracing::trace!(
        target: "cordial::nameres",
        parent_crate = ?krate,
        parent_module = ?module,
        "computing the def map of a block"
    );
    let local_def_map = crate_local_def_map(db, krate);
    let def_map = DefMap::empty(
        db,
        krate,
        local_def_map.def_map.data.clone(),
        module_data,
        Some(BlockInfo { block: block_id, parent: module }),
    );

    let (def_map, _) = collector::collect_defs(
        db,
        def_map,
        TreeId::new(ast_id.file_id, Some(block_id)),
        Some(&local_def_map.local),
    );
    let mut def_map = def_map;
    def_map.freeze(db);
    def_map
}

impl DefMap {
    pub fn edition(&self) -> Edition {
        self.data.edition
    }

    fn empty(
        db: &dyn SourceDatabase,
        krate: Crate,
        crate_data: Arc<DefMapCrateData>,
        module_data: ModuleData,
        block: Option<BlockInfo>,
    ) -> DefMap {
        let mut modules = ModulesMap::new();
        let root = unsafe {
            ModuleIdLt::new(
                db,
                krate,
                block.map(|it| it.block),
                None,
                Name::new_symbol_root(sym::__empty),
            )
            .to_static()
        };
        modules.insert(root, module_data);

        DefMap {
            block,
            root,
            modules,
            cells: FxIndexMap::default(),
            krate,
            prelude: None,
            macro_use_prelude: FxHashMap::default(),
            derive_helpers_in_scope: FxHashMap::default(),
            diagnostics: Vec::new(),
            diagnostics_cell: None,
            data: crate_data,
            macro_def_to_macro_id: FxHashMap::default(),
        }
    }
    fn shrink_to_fit(&mut self) {
        // Exhaustive match to require handling new fields.
        let Self {
            macro_use_prelude,
            diagnostics,
            diagnostics_cell: _,
            modules,
            cells,
            derive_helpers_in_scope,
            root: _,
            block: _,
            krate: _,
            prelude: _,
            data: _,
            macro_def_to_macro_id,
        } = self;

        macro_def_to_macro_id.shrink_to_fit();
        macro_use_prelude.shrink_to_fit();
        diagnostics.shrink_to_fit();
        modules.shrink_to_fit();
        cells.shrink_to_fit();
        derive_helpers_in_scope.shrink_to_fit();
        for (_, module) in modules.iter_mut() {
            module.children.shrink_to_fit();
            module.scope.shrink_to_fit();
        }
    }
}

impl DefMap {
    /// Returns all modules in the crate that are associated with the given file.
    pub fn modules_for_file<'a>(
        &'a self,
        db: &'a dyn SourceDatabase,
        file_id: FileId,
    ) -> impl Iterator<Item = ModuleId> + 'a {
        self.modules(db)
            .filter(move |(_, data)| {
                data.origin.file_id().map(|file_id| file_id.file_id(db)) == Some(file_id)
            })
            .map(|(id, _)| id)
    }

    pub fn modules<'a>(
        &'a self,
        db: &'a dyn SourceDatabase,
    ) -> impl Iterator<Item = (ModuleId, &'a ModuleData)> + 'a {
        let cells = self.cells.iter().map(move |(&id, cell)| (id, cell.data(db)));
        self.modules.iter().chain(cells)
    }

    /// The modules this map made: not the original's modules that a copy of a crate was given (see
    /// `nameres::shared`), which belong to the original and to whatever was computed for it.
    pub fn own_modules<'a>(
        &'a self,
        db: &'a dyn SourceDatabase,
    ) -> impl Iterator<Item = (ModuleId, &'a ModuleData)> + 'a {
        self.modules(db).filter(move |(id, _)| id.krate(db) == self.krate)
    }

    /// Query keys declared in this map's modules, excluding imports and inherited names.
    ///
    /// Used to retire module-level semantic values without scanning interned locations.
    /// The iterator reads existing facts only; keys may repeat. Associated items, enum
    /// variants and declarations in body-local block maps are not enumerated here.
    /// Macro definitions come from the local definition table, not the inherited
    /// legacy namespace stored in every module scope.
    fn declared_query_keys<'a>(&'a self, db: &'a dyn SourceDatabase) -> impl Iterator<Item = salsa::Id> + 'a {
        // A module the copy was given is the original's, and a scope that declares it names it: it is not ours.
        let foreign: rustc_hash::FxHashSet<salsa::Id> = self
            .own_modules(db)
            .flat_map(|(_, module)| module.children.values())
            .filter(|child| child.krate(db) != self.krate)
            .map(|child| child.as_id())
            .collect();
        self.own_modules(db).flat_map(|(id, module)| {
            std::iter::once(id.as_id()).chain(module.scope.declared_query_keys())
        }).chain(self.macro_def_to_macro_id.values().map(|id| id.as_id()))
            .filter(move |key| !foreign.contains(key))
    }

    /// Enumerate local tree locations already recorded by collection. Empty macro
    /// expansions still have call records; imported namespaces are not tree owners.
    fn item_tree_query_keys(&self, db: &dyn SourceDatabase) -> Vec<salsa::Id> {
        let mut keys = Vec::new();
        for (_, module) in self.own_modules(db) {
            match module.origin {
                ModuleOrigin::CrateRoot { definition } => {
                    keys.push(FileItemTreeKey::new(db, HirFileId::from(definition), self.krate).as_id());
                }
                ModuleOrigin::File { definition, declaration_tree_id, .. } => {
                    keys.push(FileItemTreeKey::new(db, HirFileId::from(definition), self.krate).as_id());
                    keys.push(declaration_tree_id.query_key(db, self.krate));
                }
                ModuleOrigin::Inline { definition_tree_id, .. } => {
                    keys.push(definition_tree_id.query_key(db, self.krate));
                }
                ModuleOrigin::BlockExpr { id, block } => {
                    keys.push(TreeId::new(block.file_id, Some(id)).query_key(db, self.krate));
                }
            }
            keys.extend(module.scope.all_macro_calls().map(|id| {
                FileItemTreeKey::new(db, HirFileId::from(id), self.krate).as_id()
            }));
        }
        keys
    }

    /// Returns all inline modules (mod name { ... }) in the crate that are associated with the given macro expansion.
    pub fn inline_modules_for_macro_file<'a>(
        &'a self,
        db: &'a dyn SourceDatabase,
        file_id: MacroCallId,
    ) -> impl Iterator<Item = ModuleId> + 'a {
        self.modules(db)
            .filter(move |(_, data)| {
                matches!(
                    data.origin,
                    ModuleOrigin::Inline { definition_tree_id, .. }
                    if definition_tree_id.file_id().macro_file() == Some(file_id)
                )
            })
            .map(|(id, _)| id)
    }

    pub fn derive_helpers_in_scope(
        &self,
        id: AstId<ast::Adt>,
    ) -> Option<&[(Name, MacroId, Either<MacroCallId, BuiltinDeriveImplId>)]> {
        self.derive_helpers_in_scope.get(&id.map(|it| it.upcast())).map(Deref::deref)
    }

    pub fn registered_tools(&self) -> &[Symbol] {
        &self.data.registered_tools
    }

    #[inline]
    pub fn features(&self) -> &UnstableFeatures {
        &self.data.unstable_features
    }

    pub fn is_rustc_coherence_is_core(&self) -> bool {
        self.data.rustc_coherence_is_core
    }

    pub fn is_no_std(&self) -> bool {
        self.data.no_std || self.data.no_core
    }

    pub fn is_no_core(&self) -> bool {
        self.data.no_core
    }

    pub fn fn_as_proc_macro(&self, id: FunctionId) -> Option<ProcMacroId> {
        self.data.fn_proc_macro_mapping.get(&id).copied()
    }

    pub fn proc_macro_as_fn(&self, id: ProcMacroId) -> Option<FunctionId> {
        self.data.fn_proc_macro_mapping_back.get(&id).copied()
    }

    pub fn krate(&self) -> Crate {
        self.krate
    }

    #[inline]
    pub fn crate_root(&self, db: &dyn SourceDatabase) -> ModuleId {
        match self.block {
            Some(_) => crate_def_map(db, self.krate()).root,
            None => self.root,
        }
    }

    /// This is the same as [`Self::crate_root`] for crate def maps, but for block def maps, it
    /// returns the root block module.
    pub fn root_module_id(&self) -> ModuleId {
        self.root
    }

    /// If this `DefMap` is for a block expression, returns the module containing the block (which
    /// might again be a block, or a module inside a block).
    pub fn parent(&self) -> Option<ModuleId> {
        Some(self.block?.parent)
    }

    /// Returns the module containing `local_mod`, either the parent `mod`, or the module (or block) containing
    /// the block, if `self` corresponds to a block expression.
    pub fn containing_module(
        &self,
        db: &dyn SourceDatabase,
        local_mod: ModuleIdLt<'_>,
    ) -> Option<ModuleId> {
        match self.module_data(db, local_mod).parent {
            Some(parent) => Some(parent),
            None => self.block.map(|BlockInfo { parent, .. }| parent),
        }
    }

    /// Get a reference to the def map's diagnostics.
    pub fn diagnostics<'a>(&'a self, db: &'a dyn SourceDatabase) -> &'a [DefDiagnostic] {
        match self.diagnostics_cell {
            Some(cell) => cell.diagnostics(db).as_slice(),
            None => self.diagnostics.as_slice(),
        }
    }

    pub fn recursion_limit(&self) -> u32 {
        // 128 is the default in rustc
        self.data.recursion_limit.unwrap_or(128)
    }

    // FIXME: this can use some more human-readable format (ideally, an IR
    // even), as this should be a great debugging aid.
    pub fn dump(&self, db: &dyn SourceDatabase) -> String {
        let mut buf = String::new();
        let mut current_map = self;
        while let Some(block) = current_map.block {
            go(&mut buf, db, current_map, "(block scope)", current_map.root);
            buf.push('\n');
            current_map = block.parent.def_map(db);
        }
        go(&mut buf, db, current_map, "crate", current_map.root);
        return buf;

        fn go(
            buf: &mut String,
            db: &dyn SourceDatabase,
            map: &DefMap,
            path: &str,
            module: ModuleId,
        ) {
            format_to!(buf, "{}\n", path);

            map.module_data(db, module).scope.dump(db, buf);

            for (name, child) in map.module_data(db, module).children.iter().sorted_by(|a, b| Ord::cmp(&a.0, &b.0))
            {
                let path = format!("{path}::{}", name.display(db, Edition::LATEST));
                buf.push('\n');
                go(buf, db, map, &path, *child);
            }
        }
    }
}

impl DefMap {
    pub(crate) fn block_id(&self) -> Option<BlockId> {
        self.block.map(|block| block.block)
    }

    pub(crate) fn prelude(&self) -> Option<(ModuleId, Option<UseId>)> {
        self.prelude
    }

    pub(crate) fn macro_use_prelude(&self) -> &FxHashMap<Name, (MacroId, Option<ExternCrateId>)> {
        &self.macro_use_prelude
    }

    pub(crate) fn resolve_path(
        &self,
        local_def_map: &LocalDefMap,
        db: &dyn SourceDatabase,
        original_module: ModuleId,
        path: &ModPath,
        shadow: BuiltinShadowMode,
        expected_macro_subns: Option<MacroSubNs>,
    ) -> (PerNs, Option<usize>) {
        let res = self.resolve_path_fp_with_macro(
            local_def_map,
            db,
            ResolveMode::Other,
            original_module,
            path,
            shadow,
            expected_macro_subns,
        );
        (res.resolved_def, res.segment_index)
    }

    /// The first `Option<usize>` points at the `Enum` segment in case of `Enum::Variant`, the second
    /// points at the unresolved segments.
    pub(crate) fn resolve_path_locally(
        &self,
        local_def_map: &LocalDefMap,
        db: &dyn SourceDatabase,
        original_module: ModuleId,
        path: &ModPath,
        shadow: BuiltinShadowMode,
    ) -> (PerNs, Option<usize>, ResolvePathResultPrefixInfo) {
        let res = self.resolve_path_fp_with_macro_single(
            local_def_map,
            db,
            ResolveMode::Other,
            original_module,
            path,
            shadow,
            None, // Currently this function isn't used for macro resolution.
        );
        (res.resolved_def, res.segment_index, res.prefix_info)
    }

    /// Ascends the `DefMap` hierarchy and calls `f` with every `DefMap` and containing module.
    ///
    /// If `f` returns `Some(val)`, iteration is stopped and `Some(val)` is returned. If `f` returns
    /// `None`, iteration continues.
    pub(crate) fn with_ancestor_maps<'db, T>(
        &self,
        db: &'db dyn SourceDatabase,
        local_mod: ModuleIdLt<'db>,
        f: &mut dyn FnMut(&DefMap, ModuleIdLt<'db>) -> Option<T>,
    ) -> Option<T> {
        if let Some(it) = f(self, local_mod) {
            return Some(it);
        }
        let mut block = self.block;
        while let Some(block_info) = block {
            let parent = block_info.parent.def_map(db);
            if let Some(it) = f(parent, block_info.parent) {
                return Some(it);
            }
            block = parent.block;
        }

        None
    }
}

impl ModuleData {
    pub(crate) fn new(
        origin: ModuleOrigin,
        visibility: Visibility,
        parent: Option<ModuleId>,
    ) -> Self {
        ModuleData {
            origin,
            visibility,
            parent,
            children: Default::default(),
            scope: ItemScope::default(),
        }
    }

    /// Returns a node which defines this module. That is, a file or a `mod foo {}` with items.
    pub fn definition_source(&self, db: &dyn SourceDatabase) -> InFile<ModuleSource> {
        self.origin.definition_source(db)
    }

    /// Same as [`ModuleData::definition_source`] but only returns the file id to prevent parsing the ASt.
    pub fn definition_source_file_id(&self) -> HirFileId {
        match self.origin {
            ModuleOrigin::File { definition, .. } | ModuleOrigin::CrateRoot { definition } => {
                definition.into()
            }
            ModuleOrigin::Inline { definition_tree_id, .. } => definition_tree_id.file_id(),
            ModuleOrigin::BlockExpr { block, .. } => block.file_id,
        }
    }

    pub fn definition_source_range(&self, db: &dyn SourceDatabase) -> InFile<TextRange> {
        match &self.origin {
            &ModuleOrigin::File { definition, .. } | &ModuleOrigin::CrateRoot { definition } => {
                InFile::new(
                    definition.into(),
                    ErasedAstId::new(definition.into(), ROOT_ERASED_FILE_AST_ID).to_range(db),
                )
            }
            &ModuleOrigin::Inline { definition, definition_tree_id } => InFile::new(
                definition_tree_id.file_id(),
                AstId::new(definition_tree_id.file_id(), definition).to_range(db),
            ),
            ModuleOrigin::BlockExpr { block, .. } => InFile::new(block.file_id, block.to_range(db)),
        }
    }

    /// Returns a node which declares this module, either a `mod foo;` or a `mod foo {}`.
    /// `None` for the crate root or block.
    pub fn declaration_source(&self, db: &dyn SourceDatabase) -> Option<InFile<ast::Module>> {
        let decl = self.origin.declaration()?;
        let value = decl.to_node(db);
        Some(InFile { file_id: decl.file_id, value })
    }

    /// Returns the range which declares this module, either a `mod foo;` or a `mod foo {}`.
    /// `None` for the crate root or block.
    pub fn declaration_source_range(&self, db: &dyn SourceDatabase) -> Option<InFile<TextRange>> {
        let decl = self.origin.declaration()?;
        Some(InFile { file_id: decl.file_id, value: decl.to_range(db) })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleSource {
    SourceFile(ast::SourceFile),
    Module(ast::Module),
    BlockExpr(ast::BlockExpr),
}

impl ModuleSource {
    pub fn node(&self) -> SyntaxNode {
        match self {
            ModuleSource::SourceFile(it) => it.syntax().clone(),
            ModuleSource::Module(it) => it.syntax().clone(),
            ModuleSource::BlockExpr(it) => it.syntax().clone(),
        }
    }
}

/// See `sub_namespace_match()`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MacroSubNs {
    /// Function-like macros, suffixed with `!`.
    Bang,
    /// Macros inside attributes, i.e. attribute macros and derive macros.
    Attr,
}

pub(crate) fn macro_styles_from_id(db: &dyn SourceDatabase, macro_id: MacroId) -> MacroCallStyles {
    let expander = match macro_id {
        MacroId::Macro2Id(it) => it.lookup(db).expander,
        MacroId::MacroRulesId(it) => it.lookup(db).expander,
        MacroId::ProcMacroId(it) => {
            return match it.lookup(db).kind {
                ProcMacroKind::CustomDerive => MacroCallStyles::DERIVE,
                ProcMacroKind::Bang => MacroCallStyles::FN_LIKE,
                ProcMacroKind::Attr => MacroCallStyles::ATTR,
            };
        }
    };

    match expander {
        MacroExpander::Declarative { styles } => styles,
        // Eager macros aren't *guaranteed* to be bang macros, but they *are* all bang macros currently.
        MacroExpander::BuiltIn(_) | MacroExpander::BuiltInEager(_) => MacroCallStyles::FN_LIKE,
        MacroExpander::BuiltInAttr(_) => MacroCallStyles::ATTR,
        MacroExpander::BuiltInDerive(_) => MacroCallStyles::DERIVE,
        MacroExpander::UnimplementedBuiltIn => MacroCallStyles::all(), // Unknown.
    }
}

/// Quoted from [rustc]:
/// Macro namespace is separated into two sub-namespaces, one for bang macros and
/// one for attribute-like macros (attributes, derives).
/// We ignore resolutions from one sub-namespace when searching names in scope for another.
///
/// [rustc]: https://github.com/rust-lang/rust/blob/1.69.0/compiler/rustc_resolve/src/macros.rs#L75
fn sub_namespace_match(
    db: &dyn SourceDatabase,
    macro_id: MacroId,
    expected: Option<MacroSubNs>,
) -> bool {
    let candidate = macro_styles_from_id(db, macro_id);
    match expected {
        Some(MacroSubNs::Bang) => candidate.contains(MacroCallStyles::FN_LIKE),
        Some(MacroSubNs::Attr) => {
            candidate.contains(MacroCallStyles::ATTR) || candidate.contains(MacroCallStyles::DERIVE)
        }
        // If we aren't expecting a specific sub-namespace
        // (e.g. in `use` declarations), match any macro.
        None => true,
    }
}

/// A newtype wrapper around `FxHashMap<ModuleId, ModuleData>` that implements `IndexMut`.
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModulesMap {
    inner: FxIndexMap<ModuleId, ModuleData>,
}

impl ModulesMap {
    fn new() -> Self {
        Self { inner: FxIndexMap::default() }
    }

    fn iter(&self) -> impl Iterator<Item = (ModuleId, &ModuleData)> + '_ {
        self.inner.iter().map(|(&k, v)| (k, v))
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = (ModuleId, &mut ModuleData)> + '_ {
        self.inner.iter_mut().map(|(&k, v)| (k, v))
    }
}

impl Deref for ModulesMap {
    type Target = FxIndexMap<ModuleId, ModuleData>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ModulesMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Index<ModuleIdLt<'_>> for ModulesMap {
    type Output = ModuleData;

    fn index(&self, id: ModuleIdLt<'_>) -> &ModuleData {
        self.inner
            .get(&unsafe { id.to_static() })
            .unwrap_or_else(|| panic!("ModuleId not found in ModulesMap: {id:#?}"))
    }
}

impl IndexMut<ModuleId> for ModulesMap {
    fn index_mut(&mut self, id: ModuleId) -> &mut ModuleData {
        self.inner
            .get_mut(&id)
            .unwrap_or_else(|| panic!("ModuleId not found in ModulesMap: {id:#?}"))
    }
}
