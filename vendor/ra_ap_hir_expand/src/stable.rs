//! A hash of what a definition map says that means the same in every process.
//!
//! A definition map is made of interned ids (of functions, modules, macro calls, files), numbered
//! in the order a process happened to meet them, so the map of the same source in another process
//! is not equal to it by `==`, and a hash of its bytes says nothing across processes. What a
//! dependent can read of the map is what each id stands for: where the item is written (its file's
//! path, and the item's place in the file) and what it is. This hashes that.
//!
//! Two rules keep it sound.
//!
//! * A type that holds an id is never hashed by its derived `Hash`: another process may give the
//!   same number to something else, and the hashes would be equal for things that are not. Each
//!   type has an impl here that says what its ids stand for, and the impls destructure their types
//!   without `..`, so a field added later is a compile error until someone says what it adds.
//! * What cannot be given a meaning that survives the process makes the hash volatile (different
//!   in every process, and counted), never absent: a map hashed with something left out would be
//!   equal to a map that has it.

use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use base_db::SourceDatabase;

/// What is being hashed, and the database the ids are looked up in.
pub struct StableCx<'db> {
    pub db: &'db dyn SourceDatabase,
    hasher: std::hash::DefaultHasher,
    volatile: u32,
}

/// What a hash is, and how much of it could not be given a meaning that survives the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StableFingerprint {
    pub hash: u64,
    /// How many things in it were hashed as volatile. When it is not zero, the hash is different
    /// in every process: it is safe, and a cutoff made from it never applies.
    pub volatile: u32,
}

static NONCE: AtomicU64 = AtomicU64::new(0);

impl<'db> StableCx<'db> {
    pub fn new(db: &'db dyn SourceDatabase) -> Self {
        Self { db, hasher: std::hash::DefaultHasher::new(), volatile: 0 }
    }

    pub fn finish(self) -> StableFingerprint {
        StableFingerprint { hash: self.hasher.finish(), volatile: self.volatile }
    }

    /// A word that says which kind of thing follows, so that two different things with the same
    /// contents hash differently.
    pub fn tag(&mut self, tag: &'static str) {
        tag.hash(&mut self.hasher);
    }

    pub fn write_u32(&mut self, value: u32) {
        value.hash(&mut self.hasher);
    }

    pub fn write_u64(&mut self, value: u64) {
        value.hash(&mut self.hasher);
    }

    pub fn write_bool(&mut self, value: bool) {
        value.hash(&mut self.hasher);
    }

    /// A value that holds no id and is the same in every process, by its derived `Hash`. Only for
    /// such: a type with an id in it gets an impl that says what the id stands for.
    pub fn write_plain<T: Hash>(&mut self, value: &T) {
        value.hash(&mut self.hasher);
    }

    pub fn write_str(&mut self, value: &str) {
        value.hash(&mut self.hasher);
    }

    /// Something with no meaning that survives the process: the hash is not equal to any other
    /// process's.
    pub fn volatile(&mut self, _what: &'static str) {
        self.volatile += 1;
        let process = u64::from(std::process::id());
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos() as u64);
        self.write_u64(process);
        self.write_u64(time);
        self.write_u64(NONCE.fetch_add(1, Ordering::Relaxed));
    }

    /// The entries of something whose order means nothing (a map, a set), as `==` takes it: each
    /// is hashed on its own, and what they hash to is written in order.
    pub fn unordered<T>(
        &mut self,
        entries: impl IntoIterator<Item = T>,
        mut hash_entry: impl FnMut(&mut StableCx<'_>, T),
    ) {
        let mut parts: Vec<u64> = Vec::new();
        for entry in entries {
            let mut inner = StableCx { db: self.db, hasher: std::hash::DefaultHasher::new(), volatile: 0 };
            hash_entry(&mut inner, entry);
            self.volatile += inner.volatile;
            parts.push(inner.hasher.finish());
        }
        parts.sort_unstable();
        self.write_u64(parts.len() as u64);
        for part in parts {
            self.write_u64(part);
        }
    }

    /// The path the database holds `file` at, or `None` for a file without one.
    pub fn path_of(&self, file: span::FileId) -> Option<String> {
        let root = self.db.file_source_root(file).source_root_id(self.db);
        let source_root = self.db.source_root(root).source_root(self.db);
        source_root.path_for_file(&file).map(ToString::to_string)
    }
}

/// What a value is, said so that it means the same in every process.
pub trait StableHash {
    fn stable_hash(&self, cx: &mut StableCx<'_>);
}

impl<T: StableHash + ?Sized> StableHash for &T {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        (**self).stable_hash(cx);
    }
}

impl StableHash for () {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        cx.tag("unit");
    }
}

impl StableHash for bool {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        cx.write_bool(*self);
    }
}

impl StableHash for u32 {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        cx.write_u32(*self);
    }
}

impl StableHash for str {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        cx.write_str(self);
    }
}

impl StableHash for String {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        cx.write_str(self);
    }
}

impl<T: StableHash> StableHash for Option<T> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            None => cx.tag("none"),
            Some(value) => {
                cx.tag("some");
                value.stable_hash(cx);
            }
        }
    }
}

impl<T: StableHash> StableHash for [T] {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        cx.write_u64(self.len() as u64);
        for item in self {
            item.stable_hash(cx);
        }
    }
}

impl<T: StableHash> StableHash for Vec<T> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        self.as_slice().stable_hash(cx);
    }
}

impl<T: StableHash> StableHash for Box<[T]> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        (**self).stable_hash(cx);
    }
}

impl<T: StableHash> StableHash for thin_vec::ThinVec<T> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        self.as_slice().stable_hash(cx);
    }
}

impl<A: StableHash, B: StableHash> StableHash for (A, B) {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        self.0.stable_hash(cx);
        self.1.stable_hash(cx);
    }
}

impl<A: StableHash, B: StableHash> StableHash for either::Either<A, B> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            either::Either::Left(value) => {
                cx.tag("left");
                value.stable_hash(cx);
            }
            either::Either::Right(value) => {
                cx.tag("right");
                value.stable_hash(cx);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Files, names, ast ids and macros.
// ---------------------------------------------------------------------------------------------

use crate::{
    EditionedFileId, HirFileId, MacroCallId, MacroCallKind, MacroCallLoc, MacroDefId, MacroDefKind,
    Lookup,
    attrs::AttrId,
    files::InFileWrapper,
    name::Name,
};
use base_db::Crate;
use span::FileAstId;

impl StableHash for Name {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        cx.write_str(self.as_str());
    }
}

impl StableHash for EditionedFileId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let (file, edition) = self.unpack(cx.db);
        match cx.path_of(file) {
            Some(path) => {
                cx.tag("file");
                cx.write_str(&path);
                cx.write_plain(&edition);
            }
            None => cx.volatile("a file with no path"),
        }
    }
}

impl StableHash for Crate {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        // A crate is where its root file is, and with what edition.
        cx.tag("crate");
        self.root_file_id(cx.db).stable_hash(cx);
    }
}

impl StableHash for HirFileId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            HirFileId::FileId(file) => {
                cx.tag("real file");
                file.stable_hash(cx);
            }
            HirFileId::MacroFile(call) => {
                cx.tag("macro file");
                call.stable_hash(cx);
            }
        }
    }
}

impl StableHash for span::ErasedFileAstId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        // A number made from the item's kind and place in the file, the same in every process.
        cx.write_plain(self);
    }
}

impl<N> StableHash for FileAstId<N> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        // A number made from the item's kind and place in the file, the same in every process.
        cx.write_plain(&self.erase());
    }
}

impl<T: StableHash> StableHash for InFileWrapper<HirFileId, T> {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { file_id, value } = self;
        file_id.stable_hash(cx);
        value.stable_hash(cx);
    }
}

impl StableHash for AttrId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        // The index of an attribute among its item's.
        cx.write_plain(self);
    }
}

impl StableHash for MacroCallId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        self.lookup(cx.db).stable_hash(cx);
    }
}

impl StableHash for MacroCallLoc {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { def, krate, kind, ctxt, macro_depth } = self;
        cx.tag("macro call");
        def.stable_hash(cx);
        krate.stable_hash(cx);
        kind.stable_hash(cx);
        // A call made from code a macro wrote has a hygiene context of its own, which is an id.
        if ctxt.is_root() {
            cx.tag("root context");
        } else {
            cx.volatile("a macro call in a hygiene context that is not the root");
        }
        macro_depth.stable_hash(cx);
    }
}

impl StableHash for MacroDefId {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        let Self { krate, edition, kind, local_inner, allow_internal_unsafe } = self;
        cx.tag("macro def");
        krate.stable_hash(cx);
        cx.write_plain(edition);
        kind.stable_hash(cx);
        local_inner.stable_hash(cx);
        allow_internal_unsafe.stable_hash(cx);
    }
}

impl StableHash for MacroDefKind {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            MacroDefKind::Declarative(ast, styles) => {
                cx.tag("declarative");
                ast.stable_hash(cx);
                cx.write_plain(styles);
            }
            MacroDefKind::BuiltIn(ast, expander) => {
                cx.tag("built-in");
                ast.stable_hash(cx);
                cx.write_plain(expander);
            }
            MacroDefKind::BuiltInAttr(ast, expander) => {
                cx.tag("built-in attribute");
                ast.stable_hash(cx);
                cx.write_plain(expander);
            }
            MacroDefKind::BuiltInDerive(ast, expander) => {
                cx.tag("built-in derive");
                ast.stable_hash(cx);
                cx.write_plain(expander);
            }
            MacroDefKind::BuiltInEager(ast, expander) => {
                cx.tag("built-in eager");
                ast.stable_hash(cx);
                cx.write_plain(expander);
            }
            MacroDefKind::UnimplementedBuiltIn(ast) => {
                cx.tag("unimplemented built-in");
                ast.stable_hash(cx);
            }
            MacroDefKind::ProcMacro(ast, expander, kind) => {
                cx.tag("proc macro");
                ast.stable_hash(cx);
                cx.write_plain(expander);
                cx.write_plain(kind);
            }
        }
    }
}

impl StableHash for MacroCallKind {
    fn stable_hash(&self, cx: &mut StableCx<'_>) {
        match self {
            MacroCallKind::FnLike { ast_id, expand_to, eager } => {
                cx.tag("function-like");
                ast_id.stable_hash(cx);
                cx.write_plain(expand_to);
                // What an eager call expanded its argument to is in the items it makes; which call
                // expanded it is what says this one is the same.
                match eager {
                    None => cx.tag("not eager"),
                    Some(info) => {
                        let crate::EagerCallInfo { arg: _arg, arg_id, error: _error, span: _span } =
                            &**info;
                        cx.tag("eager");
                        arg_id.stable_hash(cx);
                    }
                }
            }
            MacroCallKind::Derive { ast_id, derive_attr_index, derive_index, derive_macro_id } => {
                cx.tag("derive");
                ast_id.stable_hash(cx);
                derive_attr_index.stable_hash(cx);
                derive_index.stable_hash(cx);
                derive_macro_id.stable_hash(cx);
            }
            MacroCallKind::Attr { ast_id, attr_args: _attr_args, censored_attr_ids } => {
                // The arguments of the attribute decide what it expands to, which is in the items
                // it makes (names and places), and not in which call this is.
                cx.tag("attribute");
                ast_id.stable_hash(cx);
                censored_attr_ids.stable_hash_attr_ids(cx);
            }
        }
    }
}

impl crate::AttrMacroAttrIds {
    fn stable_hash_attr_ids(&self, cx: &mut StableCx<'_>) {
        let ids: &[AttrId] = self;
        cx.write_u64(ids.len() as u64);
        for id in ids {
            id.stable_hash(cx);
        }
    }
}
