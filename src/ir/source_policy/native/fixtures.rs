//! Standalone policy fixtures use the same database owner as loader collection.

use crate::{
    CordialResult,
    loader::{SourceLoadView, module_path_from_src_file},
};
use std::path::Path;

/// Collect allows evidence from explicitly supplied source.
#[cfg(feature = "allows")]
#[tracing::instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_allows_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<crate::etiquettes::allows::AllowSiteRecord>> {
    SourceLoadView::collect_native_fixture(file, source, |file| {
        super::allows::collect(
            file,
            crate_root,
            &module_path_from_src_file(tree_root, file.path()),
        )
    })
}

/// Collect cfg hygiene evidence from explicitly supplied source.
#[cfg(feature = "cfg_hygiene")]
#[tracing::instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_cfg_hygiene_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<crate::etiquettes::cfg_hygiene::CfgNameOccurrence>> {
    SourceLoadView::collect_native_fixture(file, source, |file| {
        super::cfg_names::collect(
            file,
            crate_root,
            &module_path_from_src_file(tree_root, file.path()),
        )
    })
}

/// Collect cfg scatter evidence from explicitly supplied source.
#[cfg(feature = "cfg_scatter")]
#[tracing::instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_cfg_scatter_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<crate::etiquettes::cfg_scatter::CfgScatterGroup>> {
    SourceLoadView::collect_native_fixture(file, source, |file| {
        super::scatter::collect(
            file,
            crate_root,
            &module_path_from_src_file(tree_root, file.path()),
        )
    })
}

/// Collect glob imports evidence from explicitly supplied source.
#[cfg(feature = "glob_imports")]
#[tracing::instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_glob_imports_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<crate::etiquettes::glob_imports::GlobImportSiteRecord>> {
    SourceLoadView::collect_native_fixture(file, source, |file| {
        super::glob_imports::collect(
            file,
            crate_root,
            &module_path_from_src_file(tree_root, file.path()),
        )
    })
}

/// Collect inline tests evidence from explicitly supplied source.
#[cfg(feature = "inline_tests")]
#[tracing::instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_inline_tests_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<crate::etiquettes::inline_tests::InlineTestSiteRecord>> {
    SourceLoadView::collect_native_fixture(file, source, |file| {
        super::inline_tests::collect(
            file,
            crate_root,
            &module_path_from_src_file(tree_root, file.path()),
        )
    })
}

/// Collect modularity evidence from explicitly supplied source.
#[cfg(feature = "modularity")]
#[tracing::instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_modularity_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<crate::etiquettes::modularity::ModularitySiteRecord>> {
    SourceLoadView::collect_native_fixture(file, source, |file| {
        super::modularity::collect(
            file,
            crate_root,
            &module_path_from_src_file(tree_root, file.path()),
        )
    })
}

/// Collect pageantry evidence from explicitly supplied source.
#[cfg(feature = "pageantry")]
#[tracing::instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_pageantry_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<crate::etiquettes::pageantry::PageantrySourceSite>> {
    SourceLoadView::collect_native_fixture(file, source, |file| {
        super::pageantry::collect(
            file,
            crate_root,
            &module_path_from_src_file(tree_root, file.path()),
        )
    })
}

/// Collect derives evidence from explicitly supplied source.
#[cfg(feature = "derives")]
#[tracing::instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_derives_source(
    source: &str,
    file: &Path,
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<crate::ir::DeriveFileFacts> {
    SourceLoadView::collect_native_fixture(file, source, |file| {
        super::derives::collect(
            file,
            crate_root,
            &module_path_from_src_file(tree_root, file.path()),
        )
    })
}
