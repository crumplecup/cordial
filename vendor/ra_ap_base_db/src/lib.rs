//! This crate defines the basic database trait for interacting with source code using [`salsa`].
//!
//! The concrete implementation DB is defined by ide.

#![cfg_attr(feature = "in-rust-tree", feature(rustc_private))]

#[cfg(feature = "in-rust-tree")]
extern crate rustc_driver as _;

pub use salsa;
use span::TextSize;

mod change;
mod editioned_file_id;
mod input;
pub mod target;

use std::{
    cell::RefCell,
    hash::BuildHasherDefault,
    panic,
    str::FromStr as _,
    sync::{Once, atomic::AtomicUsize},
};

pub use crate::{
    change::FileChange,
    editioned_file_id::EditionedFileId,
    input::{
        BuiltCrateData, BuiltDependency, Crate, CrateBuilder, CrateBuilderId, CrateDataBuilder,
        CrateDisplayName, CrateGraphBuilder, CrateName, CrateOrigin, CratesIdMap, CratesMap,
        DependencyBuilder, Env, ExtraCrateData, LangCrateOrigin, ProcMacroLoadingError,
        ProcMacroPaths, ReleaseChannel, SourceRoot, SourceRootId, TOOL_SOURCE_ROOTS,
        UniqueCrateData,
    },
};
use dashmap::{DashMap, mapref::entry::Entry};
use rustc_hash::{FxHashSet, FxHasher};
use salsa::{Durability, Setter};
pub use semver::{BuildMetadata, Prerelease, Version, VersionReq};
use triomphe::Arc;
pub use vfs::{AbsPathBuf, AnchoredPath, AnchoredPathBuf, FileId, VfsPath, file_set::FileSet};

pub type FxIndexSet<T> = indexmap::IndexSet<T, rustc_hash::FxBuildHasher>;
pub type FxIndexMap<K, V> =
    indexmap::IndexMap<K, V, std::hash::BuildHasherDefault<rustc_hash::FxHasher>>;

#[macro_export]
macro_rules! impl_intern_key {
    ($id:ident, $loc:ident) => {
        #[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, persist)]
        #[derive(PartialOrd, Ord)]
        pub struct $id {
            #[returns(ref)]
            pub loc: $loc,
        }

        // If we derive this salsa prints the values recursively, and this causes us to blow.
        impl ::std::fmt::Debug for $id {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.debug_tuple(stringify!($id))
                    .field(&format_args!("{:04x}", self.0.index()))
                    .finish()
            }
        }
    };
}

mod file_contents;
pub use file_contents::{ColdTextError, FileContents, TextDigest, TextReference};

pub const DEFAULT_FILE_TEXT_LRU_CAP: u16 = 16;
pub const DEFAULT_PARSE_LRU_CAP: u16 = 128;
pub const DEFAULT_BORROWCK_LRU_CAP: u16 = 2024;

#[derive(Debug, Default)]
pub struct Files {
    files: Arc<DashMap<vfs::FileId, FileText, BuildHasherDefault<FxHasher>>>,
    source_roots: Arc<DashMap<SourceRootId, SourceRootInput, BuildHasherDefault<FxHasher>>>,
    file_source_roots: Arc<DashMap<vfs::FileId, FileSourceRootInput, BuildHasherDefault<FxHasher>>>,
}

/// The handles `Files` keeps outside Salsa, as the ids of the inputs they name. Saved
/// beside the database so that a restored database can find its inputs again.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct FilesDump {
    files: Vec<(vfs::FileId, u64)>,
    source_roots: Vec<(SourceRootId, u64)>,
    file_source_roots: Vec<(vfs::FileId, u64)>,
}

impl FilesDump {
    /// Each file with the number of its text input.
    pub fn texts(&self) -> impl Iterator<Item = (vfs::FileId, u64)> + '_ {
        self.files.iter().copied()
    }
}

impl Files {
    /// The handles, as input ids.
    pub fn dump(&self) -> FilesDump {
        use salsa::plumbing::AsId;
        FilesDump {
            files: self.files.iter().map(|e| (*e.key(), e.value().as_id().as_bits())).collect(),
            source_roots: self
                .source_roots
                .iter()
                .map(|e| (*e.key(), e.value().as_id().as_bits()))
                .collect(),
            file_source_roots: self
                .file_source_roots
                .iter()
                .map(|e| (*e.key(), e.value().as_id().as_bits()))
                .collect(),
        }
    }

    /// Every source root the database holds, with its id and in the order of the ids, for
    /// finding the file at a path.
    pub fn source_root_inputs(&self) -> Vec<(SourceRootId, SourceRootInput)> {
        let mut roots: Vec<_> =
            self.source_roots.iter().map(|entry| (*entry.key(), *entry.value())).collect();
        roots.sort_by_key(|(id, _)| *id);
        roots
    }

    /// Take back the handles of inputs a restored database already holds.
    pub fn restore(&self, dump: FilesDump) {
        use salsa::plumbing::FromId;
        for (key, bits) in dump.files {
            self.files.insert(key, FileText::from_id(salsa::Id::from_bits(bits)));
        }
        for (key, bits) in dump.source_roots {
            self.source_roots.insert(key, SourceRootInput::from_id(salsa::Id::from_bits(bits)));
        }
        for (key, bits) in dump.file_source_roots {
            self.file_source_roots
                .insert(key, FileSourceRootInput::from_id(salsa::Id::from_bits(bits)));
        }
    }

    pub fn file_text(&self, file_id: vfs::FileId) -> FileText {
        match self.files.get(&file_id) {
            Some(text) => *text,
            None => {
                panic!("Unable to fetch file text for `vfs::FileId`: {file_id:?}; this is a bug")
            }
        }
    }

    pub fn set_file_text(&self, db: &mut dyn SourceDatabase, file_id: vfs::FileId, text: &str) {
        match self.files.entry(file_id) {
            Entry::Occupied(mut occupied) => {
                // Salsa does not compare a new input with the old one; an unchanged
                // text must not invalidate what read it.
                if !occupied.get().contents(db).is(&*text) {
                    occupied.get_mut().set_contents(db).to(FileContents::resident(Arc::from(text)));
                }
            }
            Entry::Vacant(vacant) => {
                let text = FileText::new(db, FileContents::resident(Arc::from(text)), file_id);
                vacant.insert(text);
            }
        };
    }

    /// Give the file the text `text` itself, not a copy of it: files that have the same text (a
    /// cfg context's copies of a file) then hold one allocation between them. An unchanged text
    /// is left as it is, so that what read it is not invalidated.
    pub fn set_file_text_arc(
        &self,
        db: &mut dyn SourceDatabase,
        file_id: vfs::FileId,
        text: Arc<str>,
    ) {
        match self.files.entry(file_id) {
            Entry::Occupied(mut occupied) => {
                if !occupied.get().contents(db).is(&*text) {
                    occupied.get_mut().set_contents(db).to(FileContents::resident(text));
                }
            }
            Entry::Vacant(vacant) => {
                let text = FileText::new(db, FileContents::resident(text), file_id);
                vacant.insert(text);
            }
        };
    }

    pub fn set_file_text_with_durability(
        &self,
        db: &mut dyn SourceDatabase,
        file_id: vfs::FileId,
        text: &str,
        durability: Durability,
    ) {
        match self.files.entry(file_id) {
            Entry::Occupied(mut occupied) => {
                if !occupied.get().contents(db).is(&*text) {
                    occupied
                        .get_mut()
                        .set_contents(db)
                        .with_durability(durability)
                        .to(FileContents::resident(Arc::from(text)));
                }
            }
            Entry::Vacant(vacant) => {
                let text = FileText::builder(FileContents::resident(Arc::from(text)), file_id)
                    .durability(durability)
                    .new(db);
                vacant.insert(text);
            }
        };
    }

    /// Source root of the file.
    pub fn source_root(&self, source_root_id: SourceRootId) -> SourceRootInput {
        let source_root = match self.source_roots.get(&source_root_id) {
            Some(source_root) => source_root,
            None => panic!(
                "Unable to fetch `SourceRootInput` with `SourceRootId` ({source_root_id:?}); this is a bug"
            ),
        };

        *source_root
    }

    pub fn set_source_root_with_durability(
        &self,
        db: &mut dyn SourceDatabase,
        source_root_id: SourceRootId,
        source_root: Arc<SourceRoot>,
        durability: Durability,
    ) {
        match self.source_roots.entry(source_root_id) {
            Entry::Occupied(mut occupied) => {
                if *occupied.get().source_root(db) != *source_root {
                    occupied
                        .get_mut()
                        .set_source_root(db)
                        .with_durability(durability)
                        .to(source_root);
                }
            }
            Entry::Vacant(vacant) => {
                let source_root =
                    SourceRootInput::builder(source_root).durability(durability).new(db);
                vacant.insert(source_root);
            }
        };
    }

    /// The source root of the file, or why it has none: a file no root holds is one the analysis read
    /// that is in the VFS and was never given a root. Code that asks about every file of the VFS uses
    /// this; [`Self::file_source_root`] is for rust-analyzer's own callers, which expect a root.
    pub fn try_file_source_root(
        &self,
        db: &dyn SourceDatabase,
        id: vfs::FileId,
    ) -> Result<FileSourceRootInput, NoSourceRoot> {
        self.file_source_roots.get(&id).map(|root| *root).ok_or_else(|| NoSourceRoot {
            file: id,
            path: self.path_for_file(db, id).map(|path| path.to_string()),
        })
    }

    pub fn file_source_root(
        &self,
        db: &dyn SourceDatabase,
        id: vfs::FileId,
    ) -> FileSourceRootInput {
        self.try_file_source_root(db, id).unwrap_or_else(|error| panic!("{error}"))
    }

    fn path_for_file(&self, db: &dyn SourceDatabase, id: vfs::FileId) -> Option<vfs::VfsPath> {
        for source_root in &*self.source_roots {
            let source_root = *source_root.value();
            if let Some(path) = source_root.source_root(db).path_for_file(&id) {
                return Some(path.clone());
            }
        }
        None
    }

    pub fn set_file_source_root_with_durability(
        &self,
        db: &mut dyn SourceDatabase,
        id: vfs::FileId,
        source_root_id: SourceRootId,
        durability: Durability,
    ) {
        match self.file_source_roots.entry(id) {
            Entry::Occupied(mut occupied) => {
                if occupied.get().source_root_id(db) != source_root_id {
                    occupied
                        .get_mut()
                        .set_source_root_id(db)
                        .with_durability(durability)
                        .to(source_root_id);
                }
            }
            Entry::Vacant(vacant) => {
                let file_source_root =
                    FileSourceRootInput::builder(source_root_id).durability(durability).new(db);
                vacant.insert(file_source_root);
            }
        };
    }
}

/// A file that no source root holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoSourceRoot {
    /// The file.
    pub file: vfs::FileId,
    /// Where it is, when the VFS knows.
    pub path: Option<String>,
}

impl std::fmt::Display for NoSourceRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "file {:?} (path: {}) has no source root: it is in the VFS and was never given one",
            self.file,
            self.path.as_deref().unwrap_or("<unknown>")
        )
    }
}

impl std::error::Error for NoSourceRoot {}

/// The set of roots for crates.io libraries.
/// Files in libraries are assumed to never change.
#[salsa::input(singleton, debug, persist)]
pub struct LibraryRoots {
    #[returns(ref)]
    pub roots: FxHashSet<SourceRootId>,
}

/// The set of "local" (that is, from the current workspace) roots.
/// Files in local roots are assumed to change frequently.
#[salsa::input(singleton, debug, persist)]
pub struct LocalRoots {
    #[returns(ref)]
    pub roots: FxHashSet<SourceRootId>,
}

/// What a saved state holds of a file's text: the text, or where it is on the disk and what it was
/// when the file is a library's, which does not change and is not worth keeping twice.
#[derive(serde::Serialize)]
enum SavedText<'a> {
    Text(&'a str),
    Reference(TextReference),
}

#[derive(serde::Deserialize)]
enum RestoredText {
    Text(String),
    Reference(TextReference),
}

/// Persist a file's text as a plain string, or as a reference to the file when it has one: `Arc<str>`
/// has no `Deserialize`.
fn serialize_file_text<S: serde::Serializer>(
    value: &(FileContents, vfs::FileId),
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value.0.reference() {
        Some(reference) => {
            serde::Serialize::serialize(&(SavedText::Reference(reference), value.1), serializer)
        }
        None => {
            let text = value.0.try_get().map_err(serde::ser::Error::custom)?;
            serde::Serialize::serialize(&(SavedText::Text(&text), value.1), serializer)
        }
    }
}

fn deserialize_file_text<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<(FileContents, vfs::FileId), D::Error> {
    let (text, file_id): (RestoredText, vfs::FileId) =
        serde::Deserialize::deserialize(deserializer)?;
    let contents = match text {
        RestoredText::Text(text) => FileContents::resident(Arc::from(text.as_str())),
        RestoredText::Reference(reference) => FileContents::cold(reference),
    };
    Ok((contents, file_id))
}

#[salsa::input(
    debug,
    persist(serialize = serialize_file_text, deserialize = deserialize_file_text)
)]
pub struct FileText {
    #[returns(clone)]
    pub contents: FileContents,
    pub file_id: vfs::FileId,
}

impl FileText {
    /// The text of the file, read from the disk if it was dropped from memory.
    pub fn text(self, db: &dyn salsa::Database) -> Arc<str> {
        self.contents(db).get()
    }
}

#[salsa::input(debug, persist)]
pub struct FileSourceRootInput {
    #[returns(copy)]
    pub source_root_id: SourceRootId,
}

#[salsa::input(debug, persist)]
pub struct SourceRootInput {
    #[returns(clone)]
    pub source_root: Arc<SourceRoot>,
    /// The crates a tool added whose root files are in this source root (see
    /// `TOOL_SOURCE_ROOTS`); always empty for the roots the loader made. Kept here, with the
    /// root, so that what is asked about one tool root depends on that root and on no other.
    #[returns(clone)]
    #[default]
    pub crates: std::sync::Arc<[Crate]>,
}

#[salsa::db]
pub trait SourceDatabase: salsa::Database + std::fmt::Debug {
    /// Text of the file.
    fn file_text(&self, file_id: vfs::FileId) -> FileText;

    fn set_file_text(&mut self, file_id: vfs::FileId, text: &str);

    /// [`Self::set_file_text`] with the allocation `text` is in, so files with the same text
    /// share it.
    fn set_file_text_arc(&mut self, file_id: vfs::FileId, text: Arc<str>);

    fn set_file_text_with_durability(
        &mut self,
        file_id: vfs::FileId,
        text: &str,
        durability: Durability,
    );

    /// Contents of the source root.
    fn source_root(&self, id: SourceRootId) -> SourceRootInput;

    fn file_source_root(&self, id: vfs::FileId) -> FileSourceRootInput;

    fn set_file_source_root_with_durability(
        &mut self,
        id: vfs::FileId,
        source_root_id: SourceRootId,
        durability: Durability,
    );

    /// Source root of the file.
    fn set_source_root_with_durability(
        &mut self,
        source_root_id: SourceRootId,
        source_root: Arc<SourceRoot>,
        durability: Durability,
    );

    fn resolve_path(&self, path: AnchoredPath<'_>) -> Option<FileId> {
        // FIXME: this *somehow* should be platform agnostic...
        let source_root = self.file_source_root(path.anchor);
        let source_root = self.source_root(source_root.source_root_id(self));
        source_root.source_root(self).resolve_path(path)
    }

    #[doc(hidden)]
    fn crates_map(&self) -> Arc<CratesMap>;

    fn nonce_and_revision(&self) -> (Nonce, salsa::Revision);

    fn line_column(&self, file: FileId, offset: TextSize) -> Result<(u32, u32), ()>;
}

static NEXT_NONCE: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Nonce(usize);

impl Default for Nonce {
    #[inline]
    fn default() -> Self {
        Nonce::new()
    }
}

impl Nonce {
    #[inline]
    pub const fn invalid() -> Nonce {
        Nonce(usize::MAX)
    }

    #[inline]
    pub fn new() -> Nonce {
        Nonce(NEXT_NONCE.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }
}

/// Crate related data shared by the whole workspace.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct CrateWorkspaceData {
    pub target: Result<target::TargetData, target::TargetLoadError>,
    /// Toolchain version used to compile the crate.
    pub toolchain: Option<Version>,
}

impl CrateWorkspaceData {
    pub fn is_atleast_187(&self) -> bool {
        const VERSION_187: Version = Version {
            major: 1,
            minor: 87,
            patch: 0,
            pre: Prerelease::EMPTY,
            build: BuildMetadata::EMPTY,
        };
        self.toolchain.as_ref().map_or(false, |v| *v >= VERSION_187)
    }
}

pub fn toolchain_channel(db: &dyn salsa::Database, krate: Crate) -> Option<ReleaseChannel> {
    krate.workspace_data(db).toolchain.as_ref().and_then(|v| ReleaseChannel::from_str(&v.pre).ok())
}

#[salsa::input(singleton, debug, persist)]
struct AllCrates {
    #[returns(clone)]
    crates: std::sync::Arc<[Crate]>,
}

pub fn set_all_crates_with_durability(
    db: &mut dyn salsa::Database,
    crates: impl IntoIterator<Item = Crate>,
    durability: Durability,
) {
    AllCrates::try_get(db)
        .unwrap_or_else(|| AllCrates::new(db, std::sync::Arc::default()))
        .set_crates(db)
        .with_durability(durability)
        .to(crates.into_iter().collect());
}

/// The source roots a tool added crates in, in the order it added them. Only [`all_crates`] reads
/// this: a lookup of one root's crates reads that root, so adding a root writes nothing that
/// the results already computed for another root depend on.
#[salsa::input(singleton, debug, persist)]
struct ToolRoots {
    #[returns(clone)]
    roots: std::sync::Arc<[SourceRootInput]>,
}

/// Register `crates` as the crates the tool added in source root `id` (one of the roots from
/// `TOOL_SOURCE_ROOTS` up, which must hold the crates' root files).
pub fn set_tool_root_crates(
    db: &mut dyn SourceDatabase,
    id: SourceRootId,
    crates: std::sync::Arc<[Crate]>,
    durability: Durability,
) {
    let root = db.source_root(id);
    if root.crates(db) != crates {
        root.set_crates(db).with_durability(durability).to(crates);
    }
    let known = ToolRoots::try_get(db).map(|roots| roots.roots(db));
    if known.as_ref().is_some_and(|roots| roots.contains(&root)) {
        return;
    }
    let mut roots: Vec<SourceRootInput> = known.map(|roots| roots.to_vec()).unwrap_or_default();
    roots.push(root);
    let roots: std::sync::Arc<[SourceRootInput]> = roots.into();
    match ToolRoots::try_get(db) {
        Some(existing) => {
            existing.set_roots(db).with_durability(durability).to(roots);
        }
        None => {
            let _ = ToolRoots::builder(roots).durability(durability).new(db);
        }
    }
}

/// The crates the tool added, in the order of the source roots it added them in.
pub fn tool_crates(db: &dyn salsa::Database) -> std::sync::Arc<[Crate]> {
    ToolRoots::try_get(db).map_or_else(std::sync::Arc::default, |roots| {
        roots.roots(db).iter().flat_map(|root| root.crates(db).to_vec()).collect()
    })
}

/// The crates of the graph the loader built, in topological order.
pub fn graph_crates(db: &dyn salsa::Database) -> std::sync::Arc<[Crate]> {
    AllCrates::try_get(db).map_or(std::sync::Arc::default(), |all_crates| all_crates.crates(db))
}

/// Returns the crates in topological order, then the crates a tool added.
///
/// **Warning**: do not use this query in `hir-*` crates! It kills incrementality across crate metadata modifications.
pub fn all_crates(db: &dyn salsa::Database) -> std::sync::Arc<[Crate]> {
    let graph = graph_crates(db);
    let tool = tool_crates(db);
    if tool.is_empty() { graph } else { graph.iter().chain(tool.iter()).copied().collect() }
}

// FIXME: VFS rewrite should allow us to get rid of this wrapper
#[doc(hidden)]
#[salsa::interned(revisions = usize::MAX, persist)]
pub struct InternedSourceRootId {
    #[returns(copy)]
    pub id: SourceRootId,
}

/// Crates whose root file is in `id`.
pub fn source_root_crates(db: &dyn SourceDatabase, id: SourceRootId) -> &[Crate] {
    // Persisted, so that what is saved of a memo that asks which crates a file is in depends on this
    // answer and not on the list of every crate it is made from: a project with other crates than the
    // one a state was saved from has the same answer for the files of the sysroot, and what read it
    // is not invalidated for a list that changed elsewhere.
    #[salsa::tracked(returns(deref), persist)]
    pub fn source_root_crates<'db>(
        db: &'db dyn SourceDatabase,
        id: InternedSourceRootId<'db>,
    ) -> Box<[Crate]> {
        let id = id.id(db);
        // A tool's source root holds its crates itself, so what is asked about the graph's own
        // roots depends on none of the tool's, and what is asked about one tool root depends on
        // no other.
        let crates = if id.0 >= crate::input::TOOL_SOURCE_ROOTS {
            db.source_root(id).crates(db)
        } else {
            AllCrates::get(db).crates(db)
        };
        crates
            .iter()
            .copied()
            .filter(|&krate| {
                let root_file = krate.data(db).root_file_id;
                db.file_source_root(root_file).source_root_id(db) == id
            })
            .collect()
    }
    source_root_crates(db, InternedSourceRootId::new(db, id))
}

pub fn relevant_crates(db: &dyn SourceDatabase, file_id: FileId) -> &[Crate] {
    let _p = tracing::info_span!("relevant_crates").entered();

    let source_root = db.file_source_root(file_id);
    source_root_crates(db, source_root.source_root_id(db))
}

#[must_use]
#[non_exhaustive]
pub struct DbPanicContext;

impl Drop for DbPanicContext {
    fn drop(&mut self) {
        Self::with_ctx(|ctx| assert!(ctx.pop().is_some()));
    }
}

impl DbPanicContext {
    pub fn enter(frame: String) -> DbPanicContext {
        #[expect(clippy::print_stderr, reason = "already panicking anyway")]
        fn set_hook() {
            let default_hook = panic::take_hook();
            panic::set_hook(Box::new(move |panic_info| {
                default_hook(panic_info);
                if let Some(backtrace) = salsa::Backtrace::capture() {
                    eprintln!("{backtrace:#}");
                }
                DbPanicContext::with_ctx(|ctx| {
                    if !ctx.is_empty() {
                        eprintln!("additional context:");
                        for (idx, frame) in ctx.iter().enumerate() {
                            eprintln!("{idx:>4}: {frame}\n");
                        }
                    }
                });
            }));
        }

        static SET_HOOK: Once = Once::new();
        SET_HOOK.call_once(set_hook);

        Self::with_ctx(|ctx| ctx.push(frame));
        DbPanicContext
    }

    fn with_ctx(f: impl FnOnce(&mut Vec<String>)) {
        thread_local! {
            static CTX: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        }
        CTX.with(|ctx| f(&mut ctx.borrow_mut()));
    }
}
