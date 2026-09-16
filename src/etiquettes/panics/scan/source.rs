use std::path::{Path, PathBuf};

use crate::error::CordialResult;
use crate::loader::path_has_fixtures;

use tracing::instrument;

/// One parsed source file plus the tree root it was discovered under
/// (needed to derive its crate-relative module path).
#[derive(derive_getters::Getters, derive_new::new)]
pub(super) struct ParsedFile {
    path: PathBuf,
    src_root: PathBuf,
    syntax: syn::File,
}

#[instrument(level = "debug", err(level = "warn"))]
pub(super) fn parse_source_tree(
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ParsedFile>> {
    let mut parsed = Vec::new();
    if !src_root.is_dir() {
        return Ok(parsed);
    }

    for entry in walkdir::WalkDir::new(src_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") || path_has_fixtures(path, crate_root) {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        let syntax = syn::parse_file(&source).map_err(|err| {
            crate::error::CordialError::syn_parse(path.display().to_string(), err)
        })?;
        parsed.push(ParsedFile::new(
            path.to_path_buf(),
            src_root.to_path_buf(),
            syntax,
        ));
    }

    Ok(parsed)
}
