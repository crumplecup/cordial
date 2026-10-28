use std::path::{Path, PathBuf};
use std::rc::Rc;

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
    syntax: ParsedSyntax,
}

enum ParsedSyntax {
    Shared(SharedParsedFile),
    Standalone(Rc<syn::File>),
}

impl ParsedFile {
    #[instrument(level = "debug", skip(syntax))]
    pub(super) fn shared(src_root: PathBuf, syntax: SharedParsedFile) -> Self {
        Self {
            path: syntax.path().to_path_buf(),
            src_root,
            syntax: ParsedSyntax::Shared(syntax),
        }
    }

    #[instrument(level = "debug", skip(path, syntax))]
    pub(super) fn standalone(path: PathBuf, src_root: PathBuf, syntax: Rc<syn::File>) -> Self {
        Self {
            path,
            src_root,
            syntax: ParsedSyntax::Standalone(syntax),
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub(super) fn syntax(&self) -> &syn::File {
        match &self.syntax {
            ParsedSyntax::Shared(file) => file.syntax(),
            ParsedSyntax::Standalone(syntax) => syntax,
        }
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

    for entry in walkdir::WalkDir::new(src_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
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
