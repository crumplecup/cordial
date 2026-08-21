//! syn-based scan for error-producing control-flow sites.

use std::path::Path;

use crate::error::CordialResult;
use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::loader::{path_has_fixtures, quality_scan_trees};

use tracing::instrument;
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_sites(crate_root: &Path) -> CordialResult<Vec<ErrorSiteRecord>> {
    let mut findings = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        findings.extend(scan_source_tree(&tree_root, crate_root)?);
    }

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.kind.to_string().cmp(&b.kind.to_string()))
            .then(a.source_snippet.cmp(&b.source_snippet))
    });

    Ok(findings)
}

#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorSiteRecord>> {
    let mut findings = Vec::new();
    if !tree_root.is_dir() {
        return Ok(findings);
    }

    for entry in walkdir::WalkDir::new(tree_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        if path_has_fixtures(path, crate_root) {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        findings.extend(scan_rust_source(&source, path, tree_root, crate_root)?);
    }

    Ok(findings)
}

#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    Ok(scan_rust_syntax(&syntax, file, tree_root, crate_root))
}

/// Scan a pre-parsed file for error sites (via unified error IR visitor).
#[instrument(level = "debug", skip(syntax, file))]
pub(crate) fn scan_rust_syntax(
    syntax: &syn::File,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> Vec<ErrorSiteRecord> {
    let src_root = crate_root.join("src");
    crate::etiquettes::scan_rust_file_syntax(
        syntax,
        file,
        tree_root,
        &src_root,
        crate_root,
        "",
        crate::etiquettes::ErrorIrScanLayers::SITES_ONLY,
    )
    .sites
}
