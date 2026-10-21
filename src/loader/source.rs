use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use crate::error::CordialResult;
use crate::hooks::{LoadContext, Loader};
use crate::ir::CrateKind;

use super::LoadView;

use tracing::instrument;
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
        let target = view.target();

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
                let source = super::source_work::read(path, Self::ID)?;
                files.push(SourceFile::new(path.to_path_buf(), source));
            }
        }

        Ok(Box::new(SourceLoadView::new(
            target.crate_name().clone(),
            src_root,
            files,
            target.kinds().clone(),
        )))
    }
}

/// Parsed source file retained for IR construction.
#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct SourceFile {
    path: PathBuf,
    source: String,
}

/// Source loader output.
#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub struct SourceLoadView {
    crate_name: String,
    src_root: PathBuf,
    files: Vec<SourceFile>,
    crate_kinds: Vec<CrateKind>,
    #[new(default)]
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

type ParsedFiles = HashMap<PathBuf, Rc<syn::File>>;
type ParsedRuns = HashMap<u64, (Weak<CacheToken>, ParsedFiles)>;

thread_local! {
    static PARSED: RefCell<ParsedRuns> = RefCell::new(HashMap::new());
}

impl SourceLoadView {
    /// Return the same parsed file to every hook scanning this path.
    /// Callers own tree discovery and fixture filtering.
    #[tracing::instrument(level = "debug", skip(self, path))]
    #[hotpath::measure(impl_type = "SourceLoadView")]
    pub(crate) fn parsed_file(&self, path: &Path, hook: &str) -> CordialResult<Rc<syn::File>> {
        PARSED.with(|parsed| {
            let mut parsed = parsed.borrow_mut();
            parsed.retain(|_, (token, _)| token.strong_count() != 0);
            let files = &mut parsed
                .entry(self.cache_token.0)
                .or_insert_with(|| (Arc::downgrade(&self.cache_token), HashMap::new()))
                .1;
            if let Some(syntax) = files.get(path) {
                super::source_work::reuse(path, hook);
                return Ok(Rc::clone(syntax));
            }
            let syntax = if let Some(file) = self.files.iter().find(|file| file.path() == path) {
                super::source_work::parse(file.source(), path, hook)?
            } else {
                let source = super::source_work::read(path, hook)?;
                super::source_work::parse(&source, path, hook)?
            };
            let syntax = Rc::new(syntax);
            files.insert(path.to_path_buf(), Rc::clone(&syntax));
            Ok(syntax)
        })
    }

    /// Drop parsed trees after the crate's enrichers have consumed them.
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn release_parsed(&self) {
        PARSED.with(|parsed| {
            parsed.borrow_mut().remove(&self.cache_token.0);
        });
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
