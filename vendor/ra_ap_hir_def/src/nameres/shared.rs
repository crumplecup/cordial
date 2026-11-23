//! Which modules of a crate a copy of it takes from the original as they are.
//!
//! The tool makes a copy of a crate to analyse it under another cfg. Most of the crate is the same in the copy: a
//! module is **unaffected** when nothing the context changes can reach what it is made of, and what rust-analyzer
//! computes for it (its scope, its items and everything keyed by their ids) can be the original's.
//!
//! The tool says which files it cannot take from the original (`Crate::affected_files`: the files it copied, and the
//! files that mention a name a flipped gate defines). This closes that set over what a module is made of, from the
//! original's finished map:
//!
//! - a module of an affected file is affected, and so is one defined by a macro expansion;
//! - a module whose parent is affected is not for that reason, but the parent of an affected module is: its
//!   children and what it declares change with them;
//! - a module that imports a name from an affected module is affected (a glob counts: the names it brings in are
//!   in its scope, and each says where it came from);
//! - a module that defines a macro is affected, since an exported macro reaches the crate root, and so is one that
//!   inherits textual macro scope from an affected parent;
//! - a crate that exports procedural macros is affected whole, as its map holds data that belongs to the crate.
//!
//! Whatever is left is unaffected, and closed downward: an affected child makes its parent affected.

use base_db::{Crate, SourceDatabase};
use rustc_hash::FxHashSet;

use crate::{ModuleDefId, ModuleId, nameres::crate_def_map, per_ns::PerNs};

/// The modules of the original that a copy shares and those it does not.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SharedModules {
    /// The original's modules the context can change what is made of, or that depend on one.
    pub affected: FxHashSet<ModuleId>,
    /// The original's modules the copy takes as they are.
    pub unaffected: FxHashSet<ModuleId>,
}

/// The modules of the crate that `copy` is a copy of that it takes as they are, or `None` when `copy` is not a copy.
#[salsa::tracked(returns(ref), persist)]
pub fn shared_modules(db: &dyn SourceDatabase, copy: Crate) -> Option<SharedModules> {
    let original = (*copy.canonical(db))?;
    let map = crate_def_map(db, original);
    let mut affected: FxHashSet<ModuleId> = FxHashSet::default();
    if copy.data(db).is_proc_macro {
        affected.extend(map.modules().map(|(id, _)| id));
    }
    let directly: FxHashSet<_> = copy.affected_files(db).iter().copied().collect();
    // What each module imports from.
    let mut imports: Vec<(ModuleId, Vec<ModuleId>)> = Vec::new();
    for (id, data) in map.modules() {
        let own_file = data.definition_source_file_id().file_id().map(|file| file.file_id(db));
        if own_file.is_none_or(|file| directly.contains(&file)) {
            affected.insert(id);
        }
        // A macro defined here can be exported to the root, or reach modules declared after it.
        if data.scope.declarations().any(|def| matches!(def, ModuleDefId::MacroId(_))) {
            affected.insert(id);
        }
        let mut from: Vec<ModuleId> = Vec::new();
        for (_, per_ns) in data.scope.entries() {
            let PerNs { types, values, macros } = per_ns;
            let imported = types
                .filter(|item| item.import.is_some())
                .map(|item| item.def)
                .into_iter()
                .chain(values.filter(|item| item.import.is_some()).map(|item| item.def))
                .chain(macros.filter(|item| item.import.is_some()).map(|item| ModuleDefId::MacroId(item.def)));
            from.extend(imported.filter_map(|def| def.module(db)).filter(|module| *module != id));
        }
        imports.push((id, from));
    }
    loop {
        let before = affected.len();
        for (id, from) in &imports {
            if affected.contains(id) {
                if let Some(parent) = map.modules[*id].parent {
                    affected.insert(parent);
                }
            } else if from.iter().any(|module| affected.contains(module)) {
                affected.insert(*id);
            } else if let Some(parent) = map.modules[*id].parent
                && affected.contains(&parent)
                && map.modules[*id].scope.legacy_macros().next().is_some()
            {
                affected.insert(*id);
            }
        }
        if affected.len() == before {
            break;
        }
    }
    let unaffected: FxHashSet<ModuleId> =
        map.modules().map(|(id, _)| id).filter(|id| !affected.contains(id)).collect();
    tracing::info!(
        target: "cordial::def_map",
        modules = (affected.len() + unaffected.len()) as u64,
        affected_modules = affected.len() as u64,
        shared_modules = unaffected.len() as u64,
        "which modules a copy of a crate takes from the original"
    );
    Some(SharedModules { affected, unaffected })
}
