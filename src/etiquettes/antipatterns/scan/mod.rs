//! syn-based scan for antipattern probes (`Box<dyn Error>`, `Result<_, String>`, `&'static` struct fields, …).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use syn::visit::Visit;

use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures, quality_scan_trees};

use super::types::AntipatternSiteRecord;

use tracing::instrument;
mod preds;
mod visitor;

use visitor::{AntipatternScanVisitor, collect_local_trait_names};

pub(crate) use preds::truncate_snippet;

/// Scan every quality tree under `crate_root`, sharing one crate-local trait name set.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_trees(crate_root: &Path) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let mut parsed = Vec::new();
    let mut local_trait_names = HashSet::new();
    for tree_root in quality_scan_trees(crate_root) {
        if !tree_root.is_dir() {
            continue;
        }
        for (source, path) in rust_sources(&tree_root, crate_root)? {
            let syntax = syn::parse_file(&source).map_err(|err| {
                crate::error::CordialError::syn_parse(path.display().to_string(), err)
            })?;
            local_trait_names.extend(collect_local_trait_names(&syntax));
            parsed.push((syntax, path, tree_root.clone()));
        }
    }

    let mut findings = Vec::new();
    for (syntax, path, tree_root) in parsed {
        findings.extend(scan_parsed(
            syntax,
            &path,
            &tree_root,
            crate_root,
            &local_trait_names,
        ));
    }
    Ok(findings)
}

#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let local_trait_names = collect_local_trait_names(&syntax);
    Ok(scan_parsed(
        syntax,
        file,
        src_root,
        crate_root,
        &local_trait_names,
    ))
}

#[instrument(level = "debug", skip(syntax, local_trait_names))]
fn scan_parsed(
    syntax: syn::File,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
    local_trait_names: &HashSet<String>,
) -> Vec<AntipatternSiteRecord> {
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = AntipatternScanVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        in_trait_definition: false,
        in_foreign_trait_impl: false,
        local_trait_names,
        findings: Vec::new(),
    };
    visitor.visit_file(&syntax);
    visitor.findings
}

#[instrument(level = "debug", skip(tree_root, crate_root), err(level = "warn"))]
fn rust_sources(tree_root: &Path, crate_root: &Path) -> CordialResult<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    if !tree_root.is_dir() {
        return Ok(files);
    }
    for entry in walkdir::WalkDir::new(tree_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") || path_has_fixtures(path, crate_root) {
            continue;
        }
        files.push((std::fs::read_to_string(path)?, path.to_path_buf()));
    }
    Ok(files)
}
