use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::error::CordialResult;
use crate::loader::{SourceLoadView, path_has_fixtures};

use tracing::instrument;

/// One parsed source file plus the tree root it was discovered under
/// (needed to derive its crate-relative module path).
#[derive(derive_getters::Getters, derive_new::new)]
pub(super) struct ParsedFile {
    path: PathBuf,
    src_root: PathBuf,
    syntax: Rc<syn::File>,
}

#[instrument(level = "debug", skip(loaded), err(level = "warn"))]
pub(super) fn parse_source_tree(
    src_root: &Path,
    crate_root: &Path,
    loaded: Option<&SourceLoadView>,
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
        let syntax = if let Some(loaded) = loaded {
            loaded.parsed_file(path, "panic-inventory")?
        } else {
            let source = crate::loader::source_read(path, "panic-inventory")?;
            Rc::new(crate::loader::source_parse(
                &source,
                path,
                "panic-inventory",
            )?)
        };
        parsed.push(ParsedFile::new(
            path.to_path_buf(),
            src_root.to_path_buf(),
            syntax,
        ));
    }

    Ok(parsed)
}
