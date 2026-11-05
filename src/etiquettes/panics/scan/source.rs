use std::path::{Path, PathBuf};

use crate::error::CordialResult;
use crate::loader::{SharedParsedFile, SourceLoadView, path_has_fixtures};

use tracing::instrument;

/// One parsed source file plus the tree root it was discovered under
/// (needed to derive its crate-relative module path).
#[derive(derive_getters::Getters)]
pub(super) struct ParsedFile {
    path: PathBuf,
    src_root: PathBuf,
    #[getter(skip)]
    syntax: SharedParsedFile,
}

impl ParsedFile {
    #[instrument(level = "debug", skip(syntax))]
    pub(super) fn shared(src_root: PathBuf, syntax: SharedParsedFile) -> Self {
        Self {
            path: syntax.path().to_path_buf(),
            src_root,
            syntax,
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn snapshot(&self) -> crate::FileSnapshot {
        crate::FileSnapshot::new(self.path.clone(), self.syntax.source_digest())
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn syntax(&self) -> &syn::File {
        self.syntax.syntax()
    }
}

#[instrument(level = "debug", skip(loaded), err(level = "warn"))]
pub(super) fn parse_source_tree(
    src_root: &Path,
    crate_root: &Path,
    loaded: &SourceLoadView,
) -> CordialResult<Vec<ParsedFile>> {
    let mut parsed = Vec::new();
    if !src_root.is_dir() {
        return Ok(parsed);
    }

    for entry in walkdir::WalkDir::new(src_root).sort_by_file_name() {
        let entry = entry.map_err(anyhow::Error::new)?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") || path_has_fixtures(path, crate_root) {
            continue;
        }
        parsed.push(ParsedFile::shared(
            src_root.to_path_buf(),
            loaded.parsed_file(path, "panic-inventory")?,
        ));
    }

    Ok(parsed)
}
