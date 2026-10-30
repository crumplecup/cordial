use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::error::CordialResult;
use crate::hooks::{LoadContext, Loader};
use crate::ir::CrateKind;

use super::LoadView;

use tracing::instrument;

mod cache;
mod macros;
use cache::CacheToken;
pub(crate) use cache::ParsedSourceSession;
pub use macros::SharedMacroSyntax;
#[cfg(feature = "internal_error_chain")]
mod edit;
#[cfg(feature = "internal_error_chain")]
pub(crate) use edit::{SourceEditView, SourceSnapshot};

/// Loads Rust source files into a [`SourceLoadView`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SourceLoader;

impl SourceLoader {
    /// Stable identifier for `SourceLoader`.
    pub const ID: &'static str = "source";
    /// IR attribute key (`ir_origin`).
    pub const ATTR_IR_ORIGIN: &'static str = crate::ir::ATTR_IR_ORIGIN;
    /// Loader origin tag written onto IR nodes.
    pub const ORIGIN: &'static str = crate::ir::ORIGIN_SOURCE;
}

impl Loader for SourceLoader {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn load(&self, view: LoadContext<'_>) -> CordialResult<Box<dyn LoadView>> {
        Ok(Box::new(SourceLoadView::for_target(view.target())?))
    }
}

impl SourceLoadView {
    /// Load the same source view used by the session for a crate target.
    #[instrument(level = "debug", skip(target), err(level = "warn"))]
    pub(crate) fn for_target(target: &super::CrateTarget) -> CordialResult<Self> {
        let src_root = target.crate_root().join("src");
        Ok(Self::new(
            target.crate_name().clone(),
            src_root.clone(),
            read_source_files(&src_root)?,
            target.kinds().clone(),
        ))
    }

    /// Load a standalone crate's source through the same snapshot path.
    #[cfg(feature = "crate_attrs")]
    #[instrument(level = "debug", skip(crate_root), err(level = "warn"))]
    pub(crate) fn for_crate_root(crate_root: &Path, crate_name: &str) -> CordialResult<Self> {
        let src_root = crate_root.join("src");
        Ok(Self::new(
            crate_name.to_string(),
            src_root.clone(),
            read_source_files(&src_root)?,
            Vec::new(),
        ))
    }
}

#[instrument(level = "debug", skip(src_root), err(level = "warn"))]
fn read_source_files(src_root: &Path) -> CordialResult<Vec<SourceFile>> {
    let mut files = Vec::new();
    if src_root.is_dir() {
        for entry in walkdir::WalkDir::new(src_root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let source = super::source_work::read(path, SourceLoader::ID)?;
            files.push(SourceFile::new(path.to_path_buf(), source));
        }
    }
    Ok(files)
}

/// Parsed source file retained for IR construction.
#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct SourceFile {
    path: PathBuf,
    source: String,
}

/// Source loader output.
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct SourceLoadView {
    crate_name: String,
    src_root: PathBuf,
    files: Vec<SourceFile>,
    crate_kinds: Vec<CrateKind>,
    #[getter(skip)]
    cache_token: Arc<CacheToken>,
}

/// One source snapshot and its syntax tree, issued by a source view.
///
/// The private fields keep syntax, text, path, and session provenance together.
/// A scanner cannot construct this from an independently parsed tree.
///
/// ```compile_fail
/// use cordial::SharedParsedFile;
/// let syntax = syn::parse_file("fn example() {}").unwrap();
/// let _: SharedParsedFile = syntax.into();
/// ```
///
/// Callers also cannot reach inside the carrier to pair another file's syntax
/// or path with this source snapshot.
///
/// ```compile_fail
/// use cordial::SharedParsedFile;
/// fn replace_snapshot(file: &SharedParsedFile) {
///     let _ = &file.inner;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SharedParsedFile {
    inner: Rc<ParsedSource>,
}

#[derive(Debug)]
struct ParsedSource {
    path: PathBuf,
    source: String,
    syntax: syn::File,
    digest: [u8; 32],
    session: u64,
    macros: std::cell::OnceCell<macros::MacroTrees>,
}

impl SharedParsedFile {
    /// Path used to load this source file.
    #[instrument(level = "trace", skip(self))]
    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    /// Source text parsed into this file's syntax tree.
    #[instrument(level = "trace", skip(self))]
    pub fn source(&self) -> &str {
        &self.inner.source
    }

    /// Digest of the exact source text that produced this syntax tree.
    #[instrument(level = "trace", skip(self))]
    pub fn source_digest(&self) -> [u8; 32] {
        self.inner.digest
    }

    /// Whether the file still contains the bytes used to build this snapshot.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    pub fn matches_disk(&self) -> CordialResult<bool> {
        matches_snapshot(&self.inner)
    }

    /// Syntax tree parsed from this file's source text.
    #[instrument(level = "trace", skip(self))]
    pub fn syntax(&self) -> &syn::File {
        &self.inner.syntax
    }

    /// Rust islands inside supported macros, parsed once with this snapshot.
    /// Unknown macro grammars return `None`; this does not expand macros.
    #[instrument(level = "trace", skip(self, invocation))]
    pub fn macro_syntax(&self, invocation: &syn::Macro) -> Option<&SharedMacroSyntax> {
        self.inner
            .macros
            .get_or_init(|| macros::collect(&self.inner.syntax))
            .get(&macros::location(invocation))
    }
}

#[instrument(level = "debug", skip(snapshot), err(level = "warn"))]
fn matches_snapshot(snapshot: &ParsedSource) -> CordialResult<bool> {
    let current = std::fs::read(&snapshot.path)?;
    let digest: [u8; 32] = Sha256::digest(current).into();
    Ok(digest == snapshot.digest)
}

impl LoadView for SourceLoadView {
    #[instrument(level = "trace", skip(self))]
    fn loader_id(&self) -> &str {
        SourceLoader::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    #[instrument(level = "trace", skip(self))]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
