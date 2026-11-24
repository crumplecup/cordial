//! Whether the definition map, as it is now, declares an item.
//!
//! An item's id holds the module it was declared in, and outlives the cfg that declared it: flip an atom, and a
//! module or a function that exists only under it has an id that still names it after the atom is flipped back.
//! What reads such an id reads a module that is deleted, and the answer is wrong or it is a panic. This asks
//! first, and says in a type what is the matter.

use salsa::Existence;

use crate::{FunctionId, ItemContainerId, ModuleDefId, ModuleId};
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
}

impl Declaration {
    /// Whether the item is declared.
    pub fn is_declared(self) -> bool {
        self == Self::Declared
    }
}

/// Whether `module` still exists, before anything of it is read.
fn exists(db: &dyn SourceDatabase, module: ModuleId) -> Result<(), Absence> {
    match salsa::Database::existence(db, module.as_id()) {
        Existence::Exists => Ok(()),
        gone => Err(Absence::ModuleGone(gone)),
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
    let in_scope = |module: ModuleId, found: &dyn Fn(&crate::item_scope::ItemScope) -> bool| {
        exists(db, module)?;
        let map = module.def_map(db);
        match map.try_module_data(db, module) {
            Some(data) if found(&data.scope) => Ok(()),
            _ => Err(Absence::NotDeclared),
        }
    };
    match function.loc(db).container {
        ItemContainerId::ModuleId(module) => in_scope(module, &|scope| {
            scope.declarations().any(|def| def == ModuleDefId::FunctionId(function))
        }),
        ItemContainerId::ImplId(implementation) => {
            in_scope(implementation.loc(db).container, &|scope| {
                scope.impls().any(|found| found == implementation)
            })
        }
        ItemContainerId::TraitId(declared_trait) => {
            in_scope(declared_trait.loc(db).container, &|scope| {
                scope
                    .declarations()
                    .any(|def| def == ModuleDefId::TraitId(declared_trait))
            })
        }
        ItemContainerId::ExternBlockId(block) => in_scope(block.loc(db).container, &|scope| {
            scope.extern_blocks().any(|found| found == block)
        }),
    }
}
