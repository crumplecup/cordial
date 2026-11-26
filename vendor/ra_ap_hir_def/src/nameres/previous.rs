//! The map a crate made the last time, kept for the next time.
//!
//! A crate's definition map is made again whenever something it read changed, and most of it is what it was: the
//! same modules, from the same files. What the last map was made from is kept beside the database (not in a query,
//! for it is a hint and not a result: a map made without it is the same map), and the next one takes from it the
//! modules that nothing it was made of has changed, with the ids they had (see `shared`).

use base_db::{Crate, FileId, SourceDatabase};
use rustc_hash::FxHashMap;
use std::sync::Mutex;
use triomphe::Arc;

use crate::{ModuleId, item_tree::file_item_tree, nameres::{DefMap, ModuleData}};

/// The modules of other crates a map was made reading, by the module that read them, while it is made.
pub(crate) struct Externals {
    /// The module being resolved for, when the module of another crate is read.
    pub(crate) by_module: FxHashMap<ModuleId, rustc_hash::FxHashSet<ModuleId>>,
    /// What was read on behalf of no module in particular.
    pub(crate) unattributed: rustc_hash::FxHashSet<ModuleId>,
    /// The modules of the crate itself that were read while the map of the crate was made, by the module they were read
    /// for: a module that imports a name through another module of its crate has read that module, whatever the name
    /// is defined in.
    pub(crate) local: FxHashMap<ModuleId, rustc_hash::FxHashSet<ModuleId>>,
}

struct Sink {
    krate: Crate,
    current: Option<ModuleId>,
    found: Externals,
}

thread_local! {
    static SINK: std::cell::RefCell<Option<Sink>> = const { std::cell::RefCell::new(None) };
    /// How many readers are looking at what was read before, which is not a read of the map being made.
    static LOOKING_BACK: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// While alive, what is read is not noted: it is the map made before being checked, not the map being made.
struct LookingBack;

impl LookingBack {
    fn new() -> Self {
        LOOKING_BACK.with(|count| count.set(count.get() + 1));
        Self
    }
}

impl Drop for LookingBack {
    fn drop(&mut self) {
        LOOKING_BACK.with(|count| count.set(count.get() - 1));
    }
}

/// While a crate's map is made, notes the modules of other crates that are read, and for which module.
pub(crate) struct Collecting {
    saved: Option<Sink>,
    ended: bool,
}

impl Collecting {
    pub(crate) fn begin(krate: Crate) -> Self {
        let mine = Sink {
            krate,
            current: None,
            found: Externals { by_module: FxHashMap::default(), unattributed: Default::default(), local: FxHashMap::default() },
        };
        let saved = SINK.with(|sink| sink.borrow_mut().replace(mine));
        Self { saved, ended: false }
    }

    /// What was read, and the notes of whatever was being made before this are as they were.
    pub(crate) fn end(mut self) -> Externals {
        self.ended = true;
        let mine = SINK.with(|sink| std::mem::replace(&mut *sink.borrow_mut(), self.saved.take()));
        mine.map(|sink| sink.found).unwrap_or(Externals {
            by_module: FxHashMap::default(),
            unattributed: Default::default(),
            local: FxHashMap::default(),
        })
    }
}

impl Drop for Collecting {
    fn drop(&mut self) {
        if !self.ended {
            SINK.with(|sink| *sink.borrow_mut() = self.saved.take());
        }
    }
}

/// While alive, what is read of another crate is read on behalf of `module`.
pub(crate) struct Reading {
    before: Option<ModuleId>,
}

impl Reading {
    pub(crate) fn for_module(module: ModuleId) -> Self {
        let before = SINK.with(|sink| {
            sink.borrow_mut().as_mut().and_then(|sink| sink.current.replace(module))
        });
        Self { before }
    }
}

impl Drop for Reading {
    fn drop(&mut self) {
        SINK.with(|sink| {
            if let Some(sink) = sink.borrow_mut().as_mut() {
                sink.current = self.before;
            }
        });
    }
}

/// `module`, of the map of `owner`, is read. Noted when a map of another crate is being made.
pub(crate) fn note_read(owner: Crate, module: ModuleId) {
    if LOOKING_BACK.with(|count| count.get()) > 0 {
        return;
    }
    SINK.with(|sink| {
        if let Some(sink) = sink.borrow_mut().as_mut()
            && sink.krate != owner
        {
            match sink.current {
                Some(reader) => {
                    sink.found.by_module.entry(reader).or_default().insert(module);
                }
                None => {
                    sink.found.unattributed.insert(module);
                }
            }
        }
    });
}

/// `module`, of the map `owner` is making, is read: for the module whose imports are being resolved, if there is one. A
/// module that reads another module of its own crate depends on it as it depends on a glob's source, and the names it
/// imports through it are the ones that module has, not the ones the definitions say they are defined in.
pub(crate) fn note_local_read(owner: Crate, module: ModuleId) {
    if LOOKING_BACK.with(|count| count.get()) > 0 {
        return;
    }
    SINK.with(|sink| {
        if let Some(sink) = sink.borrow_mut().as_mut()
            && sink.krate == owner
            && let Some(reader) = sink.current
            && reader != module
        {
            sink.found.local.entry(reader).or_default().insert(module);
        }
    });
}

/// What a finished map cannot show of what it was made of: what a module might have read and did not find, and the
/// modules a glob import takes names from, which may have had none.
#[derive(Default)]
pub(crate) struct Reads {
    /// The modules that have something unresolved (an import, a macro call, a module file, an extern crate): a name
    /// that appears anywhere in the crate may resolve it.
    pub(crate) unresolved: rustc_hash::FxHashSet<ModuleId>,
    /// Each glob import: the module that imports, and the module it imports from.
    pub(crate) globs: Vec<(ModuleId, ModuleId)>,
    /// Each module of the crate that a module read while its names were resolved: the module that read, and the module it
    /// read. What an import through a module has is what that module has.
    pub(crate) locals: Vec<(ModuleId, ModuleId)>,
    /// The modules that read a module of another crate that has changed since. Not known when the map is made: it
    /// is said of the map made before, when the next is made.
    pub(crate) external_changed: rustc_hash::FxHashSet<ModuleId>,
    /// What the modules taken from the map made before had read of other crates: it is what that map recorded.
    pub(crate) carried_externals: FxHashMap<ModuleId, Vec<(ModuleId, usize)>>,
}

/// What a crate's map was made from.
pub(crate) struct Built {
    pub(crate) reads: Reads,
    /// The modules of other crates each module read, with the address of what each held when it was read.
    pub(crate) externals: FxHashMap<ModuleId, Vec<(ModuleId, usize)>>,
    /// The modules of other crates read for no module in particular, with what each held.
    pub(crate) unattributed: Vec<(ModuleId, usize)>,
    /// The path of each module from the root, as `crate::a::b`, and its own name. Plain data, read when the map
    /// was made: a module this map has that the next does not is deleted in the revision that makes the next, and a
    /// struct that was read in a revision cannot be deleted in it.
    pub(crate) paths: FxHashMap<ModuleId, (String, hir_expand::name::Name)>,
    /// What the map said of each module that has something to say (the diagnostics).
    pub(crate) diagnostics: FxHashMap<ModuleId, Vec<super::diagnostics::DefDiagnostic>>,
    pub(crate) map: Arc<DefMap>,
    /// What each module of the map held, as the cells hold it.
    pub(crate) modules: FxHashMap<ModuleId, Arc<ModuleData>>,
    /// The address of the item tree each file of the map was made from. A tree that is made again with the same
    /// value keeps its address, as a query that does not change keeps its value; a tree that was dropped and made
    /// again, or that changed, does not.
    pub(crate) trees: FxHashMap<FileId, usize>,
}

/// The maps each crate of a database made last.
#[derive(Default)]
pub(crate) struct Previous(Mutex<FxHashMap<Crate, std::sync::Arc<Built>>>);

/// The address of the item tree of `file` as `krate` has it.
fn tree_address(db: &dyn SourceDatabase, file: crate::EditionedFileId, krate: Crate) -> usize {
    std::ptr::from_ref(file_item_tree(db, file.into(), krate)) as usize
}

/// What the module of another crate holds now, by the address of its data: `None` when it is gone.
pub(crate) fn external_address(db: &dyn SourceDatabase, module: ModuleId) -> Option<usize> {
    if salsa::Database::existence(db, salsa::plumbing::AsId::as_id(&module)) != salsa::Existence::Exists {
        return None;
    }
    module.def_map(db).try_module_data(db, module).map(|data| std::ptr::from_ref(data) as usize)
}

/// Keep `map`, which `krate` made, for the next one.
pub(crate) fn remember(
    db: &dyn SourceDatabase,
    krate: Crate,
    map: &Arc<DefMap>,
    modules: FxHashMap<ModuleId, Arc<ModuleData>>,
    reads: Reads,
    diagnostics: FxHashMap<ModuleId, Vec<super::diagnostics::DefDiagnostic>>,
    read: Externals,
) {
    let mut reads = reads;
    reads.locals.extend(read.local.iter().flat_map(|(reader, modules)| modules.iter().map(move |module| (*reader, *module))));
    tracing::info!(
        target: "cordial::def_map",
        modules_that_read_other_crates = read.by_module.len() as u64,
        read_for_no_module = read.unattributed.len() as u64,
        "what a map read of other crates"
    );
    let snapshot = |module: ModuleId| (module, external_address(db, module).unwrap_or(0));
    let mut externals: FxHashMap<ModuleId, Vec<(ModuleId, usize)>> = read
        .by_module
        .into_iter()
        .map(|(reader, modules)| (reader, modules.into_iter().map(snapshot).collect()))
        .collect();
    externals.extend(reads.carried_externals.iter().map(|(reader, read)| (*reader, read.clone())));
    let unattributed = read.unattributed.into_iter().map(snapshot).collect();
    let mut paths = FxHashMap::default();
    for &id in modules.keys() {
        let mut names = Vec::new();
        let mut at = id;
        while let Some(parent) = at.containing_module_inside_def_map(db) {
            names.push(at.name_or_empty(db));
            at = *parent;
        }
        let own = id.name_or_empty(db);
        names.reverse();
        let path = format!(
            "crate::{}",
            names.iter().map(|name| name.as_str().to_owned()).collect::<Vec<_>>().join("::")
        );
        paths.insert(id, (path, own));
    }
    let mut trees = FxHashMap::default();
    for data in modules.values() {
        if let Some(file) = data.definition_source_file_id().file_id() {
            trees.entry(file.file_id(db)).or_insert_with(|| tree_address(db, file, krate));
        }
    }
    let built = Built { reads, externals, unattributed, paths, diagnostics, map: Arc::clone(map), modules, trees };
    let kept = salsa::plumbing::extension::<Previous, _>(db);
    kept.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(krate, std::sync::Arc::new(built));
}

/// What `krate` made last, if it made anything in this database.
pub(crate) fn recall(db: &dyn SourceDatabase, krate: Crate) -> Option<std::sync::Arc<Built>> {
    let kept = salsa::plumbing::extension::<Previous, _>(db);
    kept.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&krate).cloned()
}

impl Built {
    /// Whether anything read for no module in particular is not what it was.
    pub(crate) fn unattributed_changed(&self, db: &dyn SourceDatabase) -> bool {
        let _looking_back = LookingBack::new();
        self.unattributed.iter().any(|(module, address)| external_address(db, *module) != Some(*address))
    }

    /// The modules that read a module of another crate that is not what it was. Reading it again is what keeps the
    /// map's dependence on it, for a module that is taken and does not read it.
    pub(crate) fn read_what_changed(&self, db: &dyn SourceDatabase) -> rustc_hash::FxHashSet<ModuleId> {
        let _looking_back = LookingBack::new();
        self.externals
            .iter()
            .filter(|(_, read)| {
                read.iter().any(|(module, address)| external_address(db, *module) != Some(*address))
            })
            .map(|(reader, _)| *reader)
            .collect()
    }

    /// The modules of the map that the item trees of the files that changed no longer declare, with everything
    /// under them: a module vanishes only when the module that declared it is in a file whose tree changed, and
    /// that tree says which it still declares.
    pub(crate) fn no_longer_declared(
        &self,
        db: &dyn SourceDatabase,
        krate: Crate,
        changed: &rustc_hash::FxHashSet<FileId>,
    ) -> rustc_hash::FxHashSet<ModuleId> {
        let mut gone = rustc_hash::FxHashSet::default();
        for (&id, data) in &self.modules {
            let Some(file) = data.definition_source_file_id().file_id() else { continue };
            if !changed.contains(&file.file_id(db)) {
                continue;
            }
            let tree = file_item_tree(db, file.into(), krate);
            let still = tree.declared_modules(&self.inline_path(id)).unwrap_or_default();
            for (name, &child) in &data.children {
                if !still.contains(name) {
                    self.subtree_into(child, &mut gone);
                }
            }
        }
        gone
    }

    /// The names of the inline modules from the module that owns its file down to `id`.
    fn inline_path(&self, id: ModuleId) -> Vec<hir_expand::name::Name> {
        let mut names = Vec::new();
        let mut at = id;
        while let Some(data) = self.modules.get(&at) {
            if !matches!(data.origin, crate::nameres::ModuleOrigin::Inline { .. }) {
                break;
            }
            if let Some((_, own)) = self.paths.get(&at) {
                names.push(own.clone());
            }
            match data.parent {
                Some(parent) => at = parent,
                None => break,
            }
        }
        names.reverse();
        names
    }

    fn subtree_into(&self, module: ModuleId, into: &mut rustc_hash::FxHashSet<ModuleId>) {
        let mut pending = vec![module];
        while let Some(module) = pending.pop() {
            if into.insert(module)
                && let Some(data) = self.modules.get(&module)
            {
                pending.extend(data.children.values().copied());
            }
        }
    }

    /// The files of the map whose item tree is not the one the map was made from.
    pub(crate) fn changed_files(
        &self,
        db: &dyn SourceDatabase,
        krate: Crate,
    ) -> rustc_hash::FxHashSet<FileId> {
        let mut changed = rustc_hash::FxHashSet::default();
        for data in self.modules.values() {
            let Some(file) = data.definition_source_file_id().file_id() else { continue };
            let made_from = self.trees.get(&file.file_id(db)).copied();
            if made_from != Some(tree_address(db, file, krate)) {
                changed.insert(file.file_id(db));
            }
        }
        changed
    }
}
