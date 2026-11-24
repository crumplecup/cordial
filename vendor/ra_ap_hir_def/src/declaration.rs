//! Whether the definition map, as it is now, declares an item.
//!
//! An item's id holds the module it was declared in, and outlives the cfg that declared it: flip an atom, and a
//! module or a function that exists only under it has an id that still names it after the atom is flipped back.
//! What reads such an id reads a module that is deleted, and the answer is wrong or it is a panic. This asks
//! first, and says in a type what is the matter.

use salsa::Existence;

use crate::{BlockId, EnumId, FunctionId, ImplId, ItemContainerId, ModuleDefId, ModuleId, TraitId};
use base_db::SourceDatabase;
use salsa::plumbing::AsId;

/// Whether an item is declared by the definition map now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declaration {
    Declared,
    Absent(Absence),
}

/// Why an item is not declared by the definition map now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Absence {
    /// The module the item was declared in no longer exists: it was deleted, or its slot is another module's now.
    ModuleGone(Existence),
    /// The module exists and does not declare the item (a cfg that is not set no longer declares it).
    NotDeclared,
    /// The block expression the item, or the module it is in, is of no longer exists.
    BlockGone(Existence),
}

impl Declaration {
    /// Whether the item is declared.
    pub fn is_declared(self) -> bool {
        self == Self::Declared
    }
}

/// Whether `module` still exists, before anything of it is read, and the block it is a module of, if it is
/// one: the block's own module is of the map of the block, which is made when something asks for it.
fn exists(db: &dyn SourceDatabase, module: ModuleId) -> Result<(), Absence> {
    match salsa::Database::existence(db, module.as_id()) {
        Existence::Exists => {}
        gone => return Err(Absence::ModuleGone(gone)),
    }
    if let Some(block) = module.block(db) {
        // SAFETY: the module and the block are of this database, and are not kept.
        let block = unsafe { block.to_static() };
        block_exists(db, block)?;
    }
    Ok(())
}

/// Whether `block` and the module it is in still exist.
fn block_exists(db: &dyn SourceDatabase, block: BlockId) -> Result<(), Absence> {
    match salsa::Database::existence(db, block.as_id()) {
        Existence::Exists => {}
        gone => return Err(Absence::BlockGone(gone)),
    }
    // SAFETY: the block is of this database, and the module is not kept.
    let parent = unsafe { block.module(db).to_static() };
    exists(db, parent)
}

/// Whether the map of the block, which reads the module the block is in, can be asked for: the block and every
/// module it is in, up to the crate's, exist.
pub fn block_declaration(db: &dyn SourceDatabase, block: BlockId) -> Declaration {
    match block_exists(db, block) {
        Ok(()) => Declaration::Declared,
        Err(absence) => Declaration::Absent(absence),
    }
}

/// Whether the definition map declares the impl.
pub fn impl_declaration(db: &dyn SourceDatabase, implementation: ImplId) -> Declaration {
    let module = implementation.loc(db).container;
    match in_scope(db, module, &|scope| scope.impls().any(|found| found == implementation)) {
        Ok(()) => Declaration::Declared,
        Err(absence) => Declaration::Absent(absence),
    }
}

/// Whether the definition map declares the trait.
pub fn trait_declaration(db: &dyn SourceDatabase, declared_trait: TraitId) -> Declaration {
    let module = declared_trait.loc(db).container;
    match in_scope(db, module, &|scope| {
        scope.declarations().any(|def| def == ModuleDefId::TraitId(declared_trait))
    }) {
        Ok(()) => Declaration::Declared,
        Err(absence) => Declaration::Absent(absence),
    }
}

/// Whether the definition map declares the enum.
pub fn enum_declaration(db: &dyn SourceDatabase, declared_enum: EnumId) -> Declaration {
    let module = declared_enum.loc(db).container;
    match in_scope(db, module, &|scope| {
        scope
            .declarations()
            .any(|def| def == ModuleDefId::AdtId(crate::AdtId::EnumId(declared_enum)))
    }) {
        Ok(()) => Declaration::Declared,
        Err(absence) => Declaration::Absent(absence),
    }
}

/// Whether `module` exists and its scope, as the definition map has it now, says `found`. A module of a block
/// is only asked whether it exists: its scope is made by the block's map, which asking would run.
fn in_scope(
    db: &dyn SourceDatabase,
    module: ModuleId,
    found: &dyn Fn(&crate::item_scope::ItemScope) -> bool,
) -> Result<(), Absence> {
    exists(db, module)?;
    if module.block(db).is_some() {
        return Ok(());
    }
    match module.def_map(db).try_module_data(db, module) {
        Some(data) if found(&data.scope) => Ok(()),
        _ => Err(Absence::NotDeclared),
    }
}

/// Whether the definition map declares `function`.
pub fn function_declaration(db: &dyn SourceDatabase, function: FunctionId) -> Declaration {
    match declared(db, function) {
        Ok(()) => Declaration::Declared,
        Err(absence) => Declaration::Absent(absence),
    }
}

fn declared(db: &dyn SourceDatabase, function: FunctionId) -> Result<(), Absence> {
    match function.loc(db).container {
        ItemContainerId::ModuleId(module) => in_scope(db, module, &|scope| {
            scope.declarations().any(|def| def == ModuleDefId::FunctionId(function))
        }),
        ItemContainerId::ImplId(implementation) => in_scope(
            db,
            implementation.loc(db).container,
            &|scope| scope.impls().any(|found| found == implementation),
        ),
        ItemContainerId::TraitId(declared_trait) => in_scope(
            db,
            declared_trait.loc(db).container,
            &|scope| {
                scope
                    .declarations()
                    .any(|def| def == ModuleDefId::TraitId(declared_trait))
            },
        ),
        ItemContainerId::ExternBlockId(block) => in_scope(db, block.loc(db).container, &|scope| {
            scope.extern_blocks().any(|found| found == block)
        }),
    }
}

/// Makes the definition map of `block` (or finds it made), for what must have run before it is read: it is not
/// asked of a block that is not declared.
pub fn settle_block_map(db: &dyn SourceDatabase, block: BlockId) {
    if block_declaration(db, block).is_declared() {
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
