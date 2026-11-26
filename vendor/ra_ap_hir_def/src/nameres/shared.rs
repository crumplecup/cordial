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

use crate::{ModuleDefId, ModuleId, nameres::{ModuleData, crate_def_map}, per_ns::PerNs};

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
pub(super) enum Reason {
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
    /// It calls macros, and a module that defines one is in a file that changed.
    MacroCall,
    /// It has something unresolved, which a name that is new in the crate may resolve.
    Unresolved,
    /// It no longer exists, so what the map made before holds of it cannot be read.
    Gone,
    /// It read a module of another crate that is not what it was.
    External,
}

/// The modules of the crate that `copy` is a copy of that it takes as they are, or `None` when `copy` is not a copy.
#[salsa::tracked(returns(ref), persist)]
pub fn shared_modules(db: &dyn SourceDatabase, copy: Crate) -> Option<SharedModules> {
    let original = (*copy.canonical(db))?;
    let map = crate_def_map(db, original);
    let directly: FxHashSet<_> = copy.affected_files(db).iter().copied().collect();
    let modules: rustc_hash::FxHashMap<ModuleId, &ModuleData> = map.modules(db).collect();
    let why = close_over(db, &modules, &directly, copy.data(db).is_proc_macro, &super::previous::Reads::default(), &FxHashSet::default());
    Some(split(db, &modules, why))
}

/// The modules of `map` that are affected and those that are not, given the ones the closure reached.
pub(super) fn split(
    db: &dyn SourceDatabase,
    modules: &rustc_hash::FxHashMap<ModuleId, &ModuleData>,
    why: rustc_hash::FxHashMap<ModuleId, Reason>,
) -> SharedModules {
    let affected: FxHashSet<ModuleId> = why.keys().copied().collect();
    let unaffected: FxHashSet<ModuleId> =
        modules.keys().copied().filter(|id| !affected.contains(id)).collect();
    report(db, modules.len(), &why, unaffected.len(), &|module| path_of(db, module));
    SharedModules { affected, unaffected }
}

/// Close what is affected over what `map` is made of: from the modules in the files in `directly`, through the
/// imports, the parents and the macros of the finished map.
pub(super) fn close_over(
    db: &dyn SourceDatabase,
    modules: &rustc_hash::FxHashMap<ModuleId, &ModuleData>,
    directly: &FxHashSet<base_db::FileId>,
    is_proc_macro: bool,
    reads: &super::previous::Reads,
    gone: &FxHashSet<ModuleId>,
) -> rustc_hash::FxHashMap<ModuleId, Reason> {
    let mut why: rustc_hash::FxHashMap<ModuleId, Reason> = rustc_hash::FxHashMap::default();
    for id in gone {
        why.insert(*id, Reason::Gone);
    }
    for id in &reads.external_changed {
        why.entry(*id).or_insert(Reason::External);
    }
    if is_proc_macro {
        for id in modules.keys() {
            why.entry(*id).or_insert(Reason::ProcMacro);
        }
    }
    // What each module imports from.
    let mut imports: Vec<(ModuleId, Vec<ModuleId>)> = Vec::new();
    let mut macro_definition_changed = false;
    let mut calls_macros: Vec<ModuleId> = Vec::new();
    for (&id, &data) in modules {
        let own_file = data.definition_source_file_id().file_id().map(|file| file.file_id(db));
        if own_file.is_none_or(|file| directly.contains(&file)) {
            why.entry(id).or_insert(Reason::File);
        }
        // A macro defined here can be exported to the root, or reach modules declared after it. A `macro_rules!`
        // is in the textual macro scope and not among the declarations: a module's scope holds the macros its
        // parent had when it was declared and the ones it defines, so the ones the parent does not have are its own.
        let inherited = data.parent.map(|parent| &modules[&parent].scope);
        let defines_macro = data.scope.legacy_macros().any(|(_, macros)| {
            macros
                .iter()
                .any(|mac| inherited.is_none_or(|parent| !parent.legacy_macros().any(|(_, held)| held.contains(mac))))
        }) || data.scope.declarations().any(|def| matches!(def, ModuleDefId::MacroId(_)));
        if defines_macro {
            macro_definition_changed |= matches!(why.get(&id), Some(Reason::File));
            why.entry(id).or_insert(Reason::Macro);
        }
        if data.scope.all_macro_calls().next().is_some() {
            calls_macros.push(id);
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
    // What the finished map cannot show: a macro that changed reaches every module that calls macros, and a name
    // that is new may resolve what was not resolved.
    if macro_definition_changed {
        for id in calls_macros {
            why.entry(id).or_insert(Reason::MacroCall);
        }
    }
    if why.values().any(|reason| matches!(reason, Reason::File)) {
        for id in &reads.unresolved {
            why.entry(*id).or_insert(Reason::Unresolved);
        }
    }
    loop {
        let before = why.len();
        for (importer, source) in reads.globs.iter().chain(&reads.locals) {
            if why.contains_key(source) {
                why.entry(*importer).or_insert(Reason::Import(*source));
            }
        }
        for (id, from) in &imports {
            if why.contains_key(id) {
                if let Some(parent) = modules[id].parent {
                    why.entry(parent).or_insert(Reason::Parent(*id));
                }
            } else if let Some(module) = from.iter().find(|module| why.contains_key(module)) {
                why.insert(*id, Reason::Import(*module));
            } else if let Some(parent) = modules[id].parent
                && why.contains_key(&parent)
                && modules[id].scope.legacy_macros().next().is_some()
            {
                why.insert(*id, Reason::InheritedMacros);
            }
        }
        if why.len() == before {
            break;
        }
    }
    why
}

/// The module's place in its crate, as the names from the root.
pub(super) fn path_of(db: &dyn SourceDatabase, module: ModuleId) -> String {
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
pub(super) fn report(
    _db: &dyn SourceDatabase,
    modules: usize,
    why: &rustc_hash::FxHashMap<ModuleId, Reason>,
    shared: usize,
    path_of: &dyn Fn(ModuleId) -> String,
) {
    let mut counts = [0_u64; 10];
    let mut examples: [Vec<String>; 10] = Default::default();
    let mut ordered: Vec<(&ModuleId, &Reason)> = why.iter().collect();
    ordered.sort_by_key(|(id, _)| path_of(**id));
    for (id, reason) in ordered {
        let (slot, detail) = match reason {
            Reason::File => (0, String::new()),
            Reason::Macro => (1, String::new()),
            Reason::ProcMacro => (2, String::new()),
            Reason::Import(from) => (3, format!(" imports from {}", path_of(*from))),
            Reason::Parent(child) => (4, format!(" is above {}", path_of(*child))),
            Reason::InheritedMacros => (5, String::new()),
            Reason::MacroCall => (6, String::new()),
            Reason::Unresolved => (7, String::new()),
            Reason::Gone => (8, String::new()),
            Reason::External => (9, String::new()),
        };
        counts[slot] += 1;
        if examples[slot].len() < 3 {
            examples[slot].push(format!("{}{detail}", path_of(*id)));
        }
    }
    let names = ["file", "macro", "proc_macro", "import", "parent", "inherited_macros", "macro_call", "unresolved", "gone", "external"];
    let shown: Vec<String> = names
        .iter()
        .zip(&examples)
        .filter(|(_, found)| !found.is_empty())
        .map(|(name, found)| format!("{name}: {}", found.join("; ")))
        .collect();
    tracing::info!(
        target: "cordial::def_map",
        modules = modules as u64,
        affected_modules = why.len() as u64,
        shared_modules = shared as u64,
        affected_by_file = counts[0],
        affected_by_macro = counts[1],
        affected_by_proc_macro = counts[2],
        affected_by_import = counts[3],
        affected_by_parent = counts[4],
        affected_by_inherited_macros = counts[5],
        affected_by_macro_call = counts[6],
        affected_by_unresolved = counts[7],
        affected_by_gone = counts[8],
        affected_by_external = counts[9],
        examples = %shown.join(" | "),
        "which modules a copy of a crate takes from the original, and why the others are its own"
    );
}
