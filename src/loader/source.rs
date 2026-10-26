use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use sha2::{Digest, Sha256};

use crate::error::CordialResult;
use crate::hooks::{LoadContext, Loader};
use crate::ir::CrateKind;

use super::LoadView;

use tracing::instrument;

/// Snapshot identity shared by collection and source-only planning views.
pub(crate) trait SourceSnapshot {
    fn path(&self) -> &Path;
    fn source_digest(&self) -> [u8; 32];
}

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
        let mut files = Vec::new();
        let src_root = target.crate_root().join("src");
        if src_root.is_dir() {
            for entry in walkdir::WalkDir::new(&src_root)
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

        Ok(Self::new(
            target.crate_name().clone(),
            src_root,
            files,
            target.kinds().clone(),
        ))
    }
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

#[derive(Debug)]
struct CacheToken(u64);

impl Default for CacheToken {
    #[tracing::instrument(level = "debug")]
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
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

/// Source snapshot available to edit planning without syntax-tree access.
#[derive(Debug, Clone)]
pub(crate) struct SourceEditView {
    inner: Rc<ParsedSource>,
}

#[derive(Debug)]
struct ParsedSource {
    path: PathBuf,
    source: String,
    syntax: syn::File,
    digest: [u8; 32],
    session: u64,
}

impl SharedParsedFile {
    /// Issue the text-only view used after collection has finished.
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn edit_view(&self) -> SourceEditView {
        SourceEditView {
            inner: Rc::clone(&self.inner),
        }
    }

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
        self.edit_view().matches_disk()
    }

    /// Syntax tree parsed from this file's source text.
    #[instrument(level = "trace", skip(self))]
    pub fn syntax(&self) -> &syn::File {
        &self.inner.syntax
    }
}

impl SourceEditView {
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn path(&self) -> &Path {
        &self.inner.path
    }

    #[instrument(level = "trace", skip(self))]
    pub(crate) fn source(&self) -> &str {
        &self.inner.source
    }

    #[instrument(level = "trace", skip(self))]
    pub(crate) fn source_digest(&self) -> [u8; 32] {
        self.inner.digest
    }

    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    pub(crate) fn matches_disk(&self) -> CordialResult<bool> {
        let current = std::fs::read(&self.inner.path)?;
        let digest: [u8; 32] = Sha256::digest(current).into();
        Ok(digest == self.inner.digest)
    }
}

impl SourceSnapshot for SharedParsedFile {
    #[instrument(level = "trace", skip(self))]
    fn path(&self) -> &Path {
        self.path()
    }

    #[instrument(level = "trace", skip(self))]
    fn source_digest(&self) -> [u8; 32] {
        self.source_digest()
    }
}

impl SourceSnapshot for SourceEditView {
    #[instrument(level = "trace", skip(self))]
    fn path(&self) -> &Path {
        self.path()
    }

    #[instrument(level = "trace", skip(self))]
    fn source_digest(&self) -> [u8; 32] {
        self.source_digest()
    }
}

impl<T: SourceSnapshot + ?Sized> SourceSnapshot for &T {
    #[instrument(level = "trace", skip(self))]
    fn path(&self) -> &Path {
        (*self).path()
    }

    #[instrument(level = "trace", skip(self))]
    fn source_digest(&self) -> [u8; 32] {
        (*self).source_digest()
    }
}

type ParsedFiles = HashMap<PathBuf, SharedParsedFile>;
type ParsedRuns = HashMap<u64, (Weak<CacheToken>, ParsedFiles)>;

thread_local! {
    static PARSED: RefCell<ParsedRuns> = RefCell::new(HashMap::new());
    static SESSION_TOKEN: RefCell<Option<Arc<CacheToken>>> = const { RefCell::new(None) };
}

/// Retain parsed files across all crate views in one load-and-probe run.
pub(crate) struct ParsedSourceSession {
    previous: Option<Arc<CacheToken>>,
    token: Arc<CacheToken>,
}

impl ParsedSourceSession {
    #[tracing::instrument(level = "debug")]
    pub(crate) fn begin() -> Self {
        let token = Arc::new(CacheToken::default());
        let previous = SESSION_TOKEN.with(|current| current.replace(Some(token.clone())));
        Self { previous, token }
    }
}

impl Drop for ParsedSourceSession {
    #[tracing::instrument(level = "trace", skip(self))]
    fn drop(&mut self) {
        #[cfg(feature = "verus_ir")]
        crate::verus_ir::release_shared_ir(self.token.0);
        PARSED.with(|parsed| {
            parsed.borrow_mut().remove(&self.token.0);
        });
        SESSION_TOKEN.with(|current| {
            current.replace(self.previous.take());
        });
    }
}

impl SourceLoadView {
    /// A lazy shared parse cache for a standalone operation outside a session.
    /// Its caller must drop the view before rescanning files it has edited.
    #[cfg(any(
        feature = "antipatterns",
        feature = "tracing",
        feature = "internal_error_chain",
        feature = "error_sites"
    ))]
    #[instrument(level = "debug")]
    pub(crate) fn for_standalone_scan() -> Self {
        Self::new(String::new(), PathBuf::new(), Vec::new(), Vec::new())
    }

    #[instrument(level = "debug", skip(files, crate_kinds), fields(crate_name = crate_name))]
    fn new(
        crate_name: String,
        src_root: PathBuf,
        files: Vec<SourceFile>,
        crate_kinds: Vec<CrateKind>,
    ) -> Self {
        Self {
            crate_name,
            src_root,
            files,
            crate_kinds,
            cache_token: SESSION_TOKEN.with(|current| {
                current
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| Arc::new(CacheToken::default()))
            }),
        }
    }

    /// Return the same parsed file to every hook scanning this path.
    /// Callers own tree discovery and fixture filtering.
    #[tracing::instrument(level = "debug", skip(self, path))]
    #[hotpath::measure(impl_type = "SourceLoadView")]
    pub fn parsed_file(&self, path: &Path, hook: &str) -> CordialResult<SharedParsedFile> {
        PARSED.with(|parsed| {
            let mut parsed = parsed.borrow_mut();
            parsed.retain(|_, (token, _)| token.strong_count() != 0);
            let files = &mut parsed
                .entry(self.cache_token.0)
                .or_insert_with(|| (Arc::downgrade(&self.cache_token), HashMap::new()))
                .1;
            if let Some(file) = files.get(path) {
                debug_assert_eq!(file.inner.session, self.cache_token.0);
                super::source_work::reuse(path, hook);
                return Ok(file.clone());
            }
            let source = if let Some(file) = self.files.iter().find(|file| file.path() == path) {
                file.source().clone()
            } else {
                super::source_work::read(path, hook)?
            };
            let syntax = super::source_work::parse(&source, path, hook)?;
            let digest = Sha256::digest(source.as_bytes()).into();
            let file = SharedParsedFile {
                inner: Rc::new(ParsedSource {
                    path: path.to_path_buf(),
                    source,
                    syntax,
                    digest,
                    session: self.cache_token.0,
                }),
            };
            files.insert(path.to_path_buf(), file.clone());
            Ok(file)
        })
    }

    /// Walk Rust files under a root and issue their shared parsed snapshots.
    /// The caller decides which paths belong to its scan.
    #[tracing::instrument(level = "debug", skip(self, include), err(level = "warn"))]
    pub fn parsed_rust_files_under(
        &self,
        root: &Path,
        hook: &str,
        mut include: impl FnMut(&Path) -> bool,
    ) -> CordialResult<Vec<SharedParsedFile>> {
        let mut files = Vec::new();
        if !root.is_dir() {
            return Ok(files);
        }
        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "rs") && include(path) {
                files.push(self.parsed_file(path, hook)?);
            }
        }
        Ok(files)
    }

    /// Identity of the run that issued this parsed-file view.
    #[cfg(any(
        feature = "derives",
        all(
            feature = "verus_ir",
            any(
                feature = "panics",
                feature = "proof_patterns",
                feature = "verus_warnings"
            )
        )
    ))]
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn session_id(&self) -> u64 {
        self.cache_token.0
    }
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
