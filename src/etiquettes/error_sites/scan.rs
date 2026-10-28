//! syn-based scan for error-producing control-flow sites.

use std::path::Path;

use crate::error::CordialResult;
use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::loader::{SourceLoadView, path_has_fixtures, quality_scan_trees};

use tracing::instrument;
/// Scan one crate for error sites.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_sites(crate_root: &Path) -> CordialResult<Vec<ErrorSiteRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    let mut findings = Vec::new();
    for tree_root in quality_scan_trees(crate_root) {
        findings.extend(scan_shared_source_tree(&source, &tree_root, crate_root)?);
    }

    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.kind().to_string().cmp(&b.kind().to_string()))
            .then(a.source_snippet().cmp(b.source_snippet()))
    });

    Ok(findings)
}

#[instrument(level = "debug", skip(source), err(level = "warn"))]
fn scan_shared_source_tree(
    source: &SourceLoadView,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorSiteRecord>> {
    let mut findings = Vec::new();
    for file in source.parsed_rust_files_under(tree_root, "error-sites-scan", |path| {
        !path_has_fixtures(path, crate_root)
    })? {
        findings.extend(scan_rust_syntax(
            file.syntax(),
            file.path(),
            tree_root,
            crate_root,
        )?);
    }

    Ok(findings)
}

/// Scan one Rust source file and return records.
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    scan_rust_syntax(&syntax, file, tree_root, crate_root)
}

/// Scan a pre-parsed file for error sites (via unified error IR visitor).
#[instrument(level = "debug", skip(syntax, file), err(level = "warn"))]
pub(crate) fn scan_rust_syntax(
    syntax: &syn::File,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorSiteRecord>> {
    let src_root = crate_root.join("src");
    Ok(crate::etiquettes::scan_rust_file_syntax(
        syntax,
        file,
        tree_root,
        &src_root,
        crate_root,
        "",
        crate::etiquettes::ErrorIrScanLayers::SITES_ONLY,
    )?
    .sites()
    .clone())
}
