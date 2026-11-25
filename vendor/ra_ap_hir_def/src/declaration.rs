//! Whether the definition map, as it is now, declares an item.
//!
//! An item's id holds the module it was declared in, and outlives the cfg that declared it: flip an atom, and a
//! module or a function that exists only under it has an id that still names it after the atom is flipped back.
//! The module's id is never deleted, so the id can always be read; what changes is that the crate's map no longer
//! has the module. A block's id is deleted with the body that made it, so that is asked before it is read. This says
//! which, in a type.

use crate::{BlockId, EnumId, FunctionId, ImplId, ItemContainerId, ModuleDefId, ModuleId, TraitId};
use base_db::SourceDatabase;
use salsa::Existence;
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

/// Whether `module` is in the definition map and its scope, as the map has it now, says `found`. A module of a
/// block is not asked: its scope is made by the block's map, which asking would run.
fn in_scope(
    db: &dyn SourceDatabase,
    module: ModuleId,
    found: &dyn Fn(&crate::item_scope::ItemScope) -> bool,
) -> Result<(), Absence> {
    if let Some(block) = module.block(db) {
        // A module of a block is not asked: its scope is made by the block's map, which asking would run.
        // SAFETY: the block is of this database, and is not kept.
        return block_exists(db, unsafe { block.to_static() });
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
