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

/// Why a module is affected, as the first rule that reached it found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reason {
    /// Its file is one the tool says the copy cannot take as it is, or it is defined by a macro expansion.
    File,
    /// It defines a macro.
    Macro,
    /// The crate exports procedural macros.
    ProcMacro,
    /// It imports from this affected module.
    Import(ModuleId),
    /// This affected module is under it.
    Parent(ModuleId),
    /// It inherits textual macro scope from an affected parent.
    InheritedMacros,
}

/// The modules of the crate that `copy` is a copy of that it takes as they are, or `None` when `copy` is not a copy.
#[salsa::tracked(returns(ref), persist)]
pub fn shared_modules(db: &dyn SourceDatabase, copy: Crate) -> Option<SharedModules> {
    let original = (*copy.canonical(db))?;
    let map = crate_def_map(db, original);
    let mut why: rustc_hash::FxHashMap<ModuleId, Reason> = rustc_hash::FxHashMap::default();
    if copy.data(db).is_proc_macro {
        for (id, _) in map.modules(db) {
            why.entry(id).or_insert(Reason::ProcMacro);
        }
    }
    let directly: FxHashSet<_> = copy.affected_files(db).iter().copied().collect();
    // What each module imports from.
    let mut imports: Vec<(ModuleId, Vec<ModuleId>)> = Vec::new();
    for (id, data) in map.modules(db) {
        let own_file = data.definition_source_file_id().file_id().map(|file| file.file_id(db));
        if own_file.is_none_or(|file| directly.contains(&file)) {
            why.entry(id).or_insert(Reason::File);
        }
        // A macro defined here can be exported to the root, or reach modules declared after it. A `macro_rules!`
        // is in the textual macro scope and not among the declarations: a module's scope holds the macros its
        // parent had when it was declared and the ones it defines, so the ones the parent does not have are its own.
        let inherited = data.parent.map(|parent| &map.module_data(db, parent).scope);
        let defines_macro = data.scope.legacy_macros().any(|(_, macros)| {
            macros
                .iter()
                .any(|mac| inherited.is_none_or(|parent| !parent.legacy_macros().any(|(_, held)| held.contains(mac))))
        }) || data.scope.declarations().any(|def| matches!(def, ModuleDefId::MacroId(_)));
        if defines_macro {
            why.entry(id).or_insert(Reason::Macro);
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
        let before = why.len();
        for (id, from) in &imports {
            if why.contains_key(id) {
                if let Some(parent) = map.module_data(db, *id).parent {
                    why.entry(parent).or_insert(Reason::Parent(*id));
                }
            } else if let Some(module) = from.iter().find(|module| why.contains_key(module)) {
                why.insert(*id, Reason::Import(*module));
            } else if let Some(parent) = map.module_data(db, *id).parent
                && why.contains_key(&parent)
                && map.module_data(db, *id).scope.legacy_macros().next().is_some()
            {
                why.insert(*id, Reason::InheritedMacros);
            }
        }
        if why.len() == before {
            break;
        }
    }
    let affected: FxHashSet<ModuleId> = why.keys().copied().collect();
    let unaffected: FxHashSet<ModuleId> =
        map.modules(db).map(|(id, _)| id).filter(|id| !affected.contains(id)).collect();
    report(db, map, &why, unaffected.len());
    Some(SharedModules { affected, unaffected })
}

/// The module's place in its crate, as the names from the root.
fn path_of(db: &dyn SourceDatabase, module: ModuleId) -> String {
    let mut names = Vec::new();
    let mut at = module;
    while let Some(parent) = at.containing_module_inside_def_map(db) {
        names.push(at.name_or_empty(db).as_str().to_owned());
        at = *parent;
    }
    names.reverse();
    format!("crate::{}", names.join("::"))
}

/// Say, as an event, how many modules are affected and why, with a few of each reason.
fn report(
    db: &dyn SourceDatabase,
    map: &crate::nameres::DefMap,
    why: &rustc_hash::FxHashMap<ModuleId, Reason>,
    shared: usize,
) {
    let mut counts = [0_u64; 6];
    let mut examples: [Vec<String>; 6] = Default::default();
    let mut ordered: Vec<(&ModuleId, &Reason)> = why.iter().collect();
    ordered.sort_by_key(|(id, _)| path_of(db, **id));
    for (id, reason) in ordered {
        let (slot, detail) = match reason {
            Reason::File => (0, String::new()),
            Reason::Macro => (1, String::new()),
            Reason::ProcMacro => (2, String::new()),
            Reason::Import(from) => (3, format!(" imports from {}", path_of(db, *from))),
            Reason::Parent(child) => (4, format!(" is above {}", path_of(db, *child))),
            Reason::InheritedMacros => (5, String::new()),
        };
        counts[slot] += 1;
        if examples[slot].len() < 3 {
            examples[slot].push(format!("{}{detail}", path_of(db, *id)));
        }
    }
    let names = ["file", "macro", "proc_macro", "import", "parent", "inherited_macros"];
    let shown: Vec<String> = names
        .iter()
        .zip(&examples)
        .filter(|(_, found)| !found.is_empty())
        .map(|(name, found)| format!("{name}: {}", found.join("; ")))
        .collect();
    tracing::info!(
        target: "cordial::def_map",
        modules = map.modules(db).count() as u64,
        affected_modules = why.len() as u64,
        shared_modules = shared as u64,
        affected_by_file = counts[0],
        affected_by_macro = counts[1],
        affected_by_proc_macro = counts[2],
        affected_by_import = counts[3],
        affected_by_parent = counts[4],
        affected_by_inherited_macros = counts[5],
        examples = %shown.join(" | "),
        "which modules a copy of a crate takes from the original, and why the others are its own"
    );
}
