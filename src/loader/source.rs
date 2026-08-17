use std::path::PathBuf;

use crate::error::CordialResult;
use crate::hooks::Loader;
use crate::session::SessionView;

use super::{CrateTarget, LoadView};

/// Loads Rust source files into a [`SourceLoadView`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SourceLoader;

impl SourceLoader {
    pub const ID: &'static str = "source";
    pub const ATTR_IR_ORIGIN: &'static str = crate::ir::ATTR_IR_ORIGIN;
    pub const ORIGIN: &'static str = crate::ir::ORIGIN_SOURCE;
}

impl Loader for SourceLoader {
    fn id(&self) -> &str {
        Self::ID
    }

    fn load(
        &self,
        _session: &dyn SessionView,
        target: &CrateTarget,
    ) -> CordialResult<Box<dyn LoadView>> {
        let mut files = Vec::new();
        let src_root = target.crate_root.join("src");
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
                files.push(SourceFile {
                    path: path.to_path_buf(),
                    source,
                });
            }
        }

        Ok(Box::new(SourceLoadView {
            crate_name: target.crate_name.clone(),
            src_root,
            files,
        }))
    }
}

/// Parsed source file retained for IR construction.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub source: String,
}

/// Source loader output.
#[derive(Debug, Clone)]
pub struct SourceLoadView {
    pub crate_name: String,
    pub src_root: PathBuf,
    pub files: Vec<SourceFile>,
}

impl LoadView for SourceLoadView {
    fn loader_id(&self) -> &str {
        SourceLoader::ID
    }

    fn crate_name(&self) -> &str {
        &self.crate_name
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
