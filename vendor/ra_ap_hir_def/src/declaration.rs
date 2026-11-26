//! Whether the definition map, as it is now, declares an item.
//!
//! An item's id holds the module it was declared in, and outlives the cfg that declared it: flip an atom, and a
//! module or a function that exists only under it has an id that still names it after the atom is flipped back.
//! The module's id is never deleted, so the id can always be read; what changes is that the crate's map no longer
//! has the module. A block's id is deleted with the body that made it, so that is asked before it is read. This says
//! which, in a type.

use crate::{
    BlockId, EnumId, ExternBlockId, FunctionId, ImplId, ItemContainerId, ModuleDefId, ModuleId, ModuleIdLt, TraitId,
};
use base_db::SourceDatabase;
use rustc_hash::FxHashSet;
use salsa::Existence;
use salsa::plumbing::AsId;
use triomphe::Arc;

/// Whether an item is declared by the definition map now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declaration {
    Declared,
    Absent(Absence),
}

/// Why an item is not declared by the definition map now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Absence {
    /// The map has no such module, or has it and it does not declare the item: a cfg that is not set no longer
    /// declares it.
    NotDeclared,
    /// The block expression the item, or the module it is in, is of no longer exists: the body that made it was
    /// made again without it, or forgotten.
    BlockGone(Existence),
}

impl Declaration {
    /// Whether the item is declared.
    pub fn is_declared(self) -> bool {
        self == Self::Declared
    }
}

/// The function that `ast_id` is, written in `module`, when the module's scope declares it: what mapping a syntax node to
/// its definition asks, without reading the scope as a whole. A function's id is made of where it is written (the module
/// and the node) and not of what is declared, so it is known without the scope, and whether the scope declares it is the
/// query of the function (`function_in_scope`), whose answer is the same when an unrelated name is added to the module.
/// A module of a block is not asked: its scope is made by the block's map.
pub fn declared_function(
    db: &dyn SourceDatabase,
    module: ModuleId,
    ast_id: hir_expand::AstId<syntax::ast::Fn>,
) -> Option<FunctionId> {
    use crate::{AssocItemLoc, Intern as _};
    debug_assert!(module.block(db).is_none(), "a module of a block is read through its map");
    let function = AssocItemLoc { container: ItemContainerId::ModuleId(module), id: ast_id }.intern(db);
    function_in_scope(db, function).then_some(function)
}

/// Whether `block` still exists, before anything of it is read, and the modules it is in up to the crate's.
fn block_exists(db: &dyn SourceDatabase, block: BlockId) -> Result<(), Absence> {
    match salsa::Database::existence(db, block.as_id()) {
        Existence::Exists => {}
        gone => return Err(Absence::BlockGone(gone)),
    }
    // SAFETY: the block is of this database, and the module is not kept.
    let parent = unsafe { block.module(db).to_static() };
    match parent.block(db) {
        // SAFETY: the block is of this database, and is not kept.
        Some(outer) => block_exists(db, unsafe { outer.to_static() }),
        None if parent.def_map(db).try_module_data(db, parent).is_some() => Ok(()),
        None => Err(Absence::NotDeclared),
    }
}

/// Whether the map of the block can be asked for: the block exists, and the module it is in, up to the crate's, is in
/// the crate's map.
pub fn block_declaration(db: &dyn SourceDatabase, block: BlockId) -> Declaration {
    match block_exists(db, block) {
        Ok(()) => Declaration::Declared,
        Err(absence) => Declaration::Absent(absence),
    }
}

/// What the scope of a module of a finished map declares, as sets, or `None` when the crate's map has no such module.
///
/// Asking whether one item is declared must not be reading all of the scope: a reader of the scope is out of date when
/// any name is added to it, and what asks about one function would run again for each one. The answer to each item is
/// a query of its own (below) that reads these sets and is a `bool`, which is the same after a name is added, so what
/// read it is not run again.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct DeclaredIn {
    declarations: FxHashSet<ModuleDefId>,
    impls: FxHashSet<ImplId>,
    extern_blocks: FxHashSet<ExternBlockId>,
}

#[salsa::tracked(returns(ref))]
fn module_declares<'db>(db: &'db dyn SourceDatabase, module: ModuleIdLt<'db>) -> Option<Arc<DeclaredIn>> {
    // SAFETY: the module is of this database, and is not kept.
    let module = unsafe { module.to_static() };
    module.def_map(db).try_module_data(db, module).map(|data| {
        Arc::new(DeclaredIn {
            declarations: data.scope.declarations().collect(),
            impls: data.scope.impls().collect(),
            extern_blocks: data.scope.extern_blocks().collect(),
        })
    })
}

/// The module a function's declaring scope is of, and what that scope must hold for the function to be declared.
fn container_scope(db: &dyn SourceDatabase, function: FunctionId) -> (ModuleId, Wanted) {
    match function.loc(db).container {
        ItemContainerId::ModuleId(module) => (module, Wanted::Declaration(ModuleDefId::FunctionId(function))),
        ItemContainerId::ImplId(implementation) => {
            (implementation.loc(db).container, Wanted::Impl(implementation))
        }
        ItemContainerId::TraitId(declared_trait) => {
            (declared_trait.loc(db).container, Wanted::Declaration(ModuleDefId::TraitId(declared_trait)))
        }
        ItemContainerId::ExternBlockId(block) => (block.loc(db).container, Wanted::ExternBlock(block)),
    }
}

/// What a scope has to have for an item to be declared by it.
enum Wanted {
    Declaration(ModuleDefId),
    Impl(ImplId),
    ExternBlock(ExternBlockId),
}

impl Wanted {
    fn in_(&self, declared: &DeclaredIn) -> bool {
        match self {
            Self::Declaration(def) => declared.declarations.contains(def),
            Self::Impl(implementation) => declared.impls.contains(implementation),
            Self::ExternBlock(block) => declared.extern_blocks.contains(block),
        }
    }
}

/// Whether the module's scope has what is wanted. A query of the item, so that what asks is out of date when the answer
/// changes and not when the scope does. They are persisted: a restored run compares what they say now to what they said
/// when they were saved, and a query that was not saved has nothing to be the same as, so it reads as changed.
fn scope_has(db: &dyn SourceDatabase, module: ModuleId, wanted: &Wanted) -> bool {
    module_declares(db, module).as_ref().is_some_and(|declared| wanted.in_(declared))
}

#[salsa::tracked(returns(copy), persist)]
fn function_in_scope(db: &dyn SourceDatabase, function: FunctionId) -> bool {
    let (module, wanted) = container_scope(db, function);
    scope_has(db, module, &wanted)
}

#[salsa::tracked(returns(copy), persist)]
fn impl_in_scope(db: &dyn SourceDatabase, implementation: ImplId) -> bool {
    scope_has(db, implementation.loc(db).container, &Wanted::Impl(implementation))
}

#[salsa::tracked(returns(copy), persist)]
fn trait_in_scope(db: &dyn SourceDatabase, declared_trait: TraitId) -> bool {
    scope_has(db, declared_trait.loc(db).container, &Wanted::Declaration(ModuleDefId::TraitId(declared_trait)))
}

#[salsa::tracked(returns(copy), persist)]
fn enum_in_scope(db: &dyn SourceDatabase, declared_enum: EnumId) -> bool {
    scope_has(
        db,
        declared_enum.loc(db).container,
        &Wanted::Declaration(ModuleDefId::AdtId(crate::AdtId::EnumId(declared_enum))),
    )
}

/// Whether the definition map declares the impl.
pub fn impl_declaration(db: &dyn SourceDatabase, implementation: ImplId) -> Declaration {
    declaration_of(db, implementation.loc(db).container, || impl_in_scope(db, implementation))
}

/// Whether the definition map declares the trait.
pub fn trait_declaration(db: &dyn SourceDatabase, declared_trait: TraitId) -> Declaration {
    declaration_of(db, declared_trait.loc(db).container, || trait_in_scope(db, declared_trait))
}

/// Whether the definition map declares the enum.
pub fn enum_declaration(db: &dyn SourceDatabase, declared_enum: EnumId) -> Declaration {
    declaration_of(db, declared_enum.loc(db).container, || enum_in_scope(db, declared_enum))
}

/// Whether the definition map declares `function`.
pub fn function_declaration(db: &dyn SourceDatabase, function: FunctionId) -> Declaration {
    let (module, _) = container_scope(db, function);
    declaration_of(db, module, || function_in_scope(db, function))
}

/// Whether an item in `module` is declared. A module of a block is not asked for its scope: its scope is made by the
/// block's map, which asking would run, and whether the block exists is not data a query can read, so it is said here.
fn declaration_of(db: &dyn SourceDatabase, module: ModuleId, in_scope: impl FnOnce() -> bool) -> Declaration {
    if let Some(block) = module.block(db) {
        // SAFETY: the block is of this database, and is not kept.
        return match block_exists(db, unsafe { block.to_static() }) {
            Ok(()) => Declaration::Declared,
            Err(absence) => Declaration::Absent(absence),
        };
    }
    if in_scope() { Declaration::Declared } else { Declaration::Absent(Absence::NotDeclared) }
}

/// Makes the definition map of `block` (or finds it made), for what must have run before it is read. A block in a
/// module the crate's map does not have is not asked: its map is made by looking that module up in the crate's.
pub fn settle_block_map(db: &dyn SourceDatabase, block: crate::BlockId) {
    if block_declaration(db, block).is_declared() {
        let ast_id = block.ast_id(db);
        tracing::debug!(
            target: "cordial::settle",
            block = ?block,
            in_macro_file = ast_id.file_id.is_macro(),
            module = ?block.module(db),
            "the map of a block is made"
        );
        // SAFETY: the block is of this database, and the map is not kept.
        let _ = crate::nameres::block_def_map(db, unsafe { block.to_db(db) });
    }
}

/// Makes again the body of every function, const and static the crate's definition map declares, on this thread, so
/// that a body made under a cfg that is not set is replaced before anything else runs. A body that is made again
/// deletes the blocks it made before and does not make again, and a block that another thread has read in the
/// revision cannot be deleted in it.
pub fn settle_bodies(db: &dyn SourceDatabase, krate: base_db::Crate) {
    use crate::{AssocItemId, DefWithBodyId, expr_store::Body};
    let map = crate::nameres::crate_def_map(db, krate);
    let settle = |def: DefWithBodyId| {
        let _ = Body::with_source_map(db, def);
    };
    for (_, module) in map.modules(db) {
        for def in module.scope.declarations() {
            match def {
                ModuleDefId::FunctionId(function) => settle(DefWithBodyId::FunctionId(function)),
                ModuleDefId::ConstId(constant) => settle(DefWithBodyId::ConstId(constant)),
                ModuleDefId::StaticId(item) => settle(DefWithBodyId::StaticId(item)),
                ModuleDefId::TraitId(declared) => {
                    for (_, item) in declared.trait_items(db).items.iter() {
                        match item {
                            AssocItemId::FunctionId(function) => settle(DefWithBodyId::FunctionId(*function)),
                            AssocItemId::ConstId(constant) => settle(DefWithBodyId::ConstId(*constant)),
                            AssocItemId::TypeAliasId(_) => {}
                        }
                    }
                }
                _ => {}
            }
        }
        for implementation in module.scope.impls() {
            for (_, item) in implementation.impl_items(db).items.iter() {
                match item {
                    AssocItemId::FunctionId(function) => settle(DefWithBodyId::FunctionId(*function)),
                    AssocItemId::ConstId(constant) => settle(DefWithBodyId::ConstId(*constant)),
                    AssocItemId::TypeAliasId(_) => {}
                }
            }
        }
    }
}
