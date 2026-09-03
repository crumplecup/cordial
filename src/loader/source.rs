use std::path::PathBuf;

use crate::error::CordialResult;
use crate::hooks::{LoadContext, Loader};

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
        let target = view.target;

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
                let source = std::fs::read_to_string(path)?;
                files.push(SourceFile::new(path.to_path_buf(), source));
            }
        }

        Ok(Box::new(SourceLoadView::new(
            target.crate_name().clone(),
            src_root,
            files,
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
