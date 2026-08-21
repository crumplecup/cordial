//! syn-based scan for antipattern probes (`Box<dyn Error>`, `Result<_, String>`, `&'static` struct fields except crate-local `dyn Trait` and const/static-only tables, …).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use syn::visit::Visit;

use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures, quality_scan_trees};

use super::types::AntipatternSiteRecord;

use tracing::instrument;
mod constructions;
mod preds;
mod visitor;

use constructions::{collect_constructions, types_only_constructed_in_const};
use visitor::{AntipatternScanVisitor, collect_local_trait_names};

pub(crate) use preds::truncate_snippet;

/// Scan every quality tree under `crate_root`, sharing one crate-local trait name set.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_trees(crate_root: &Path) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let mut parsed = Vec::new();
    let mut local_trait_names = HashSet::new();
    let mut const_constructed = HashSet::new();
    let mut runtime_constructed = HashSet::new();
    for tree_root in quality_scan_trees(crate_root) {
        if !tree_root.is_dir() {
            continue;
        }
        for (source, path) in rust_sources(&tree_root, crate_root)? {
            let syntax = syn::parse_file(&source).map_err(|err| {
                crate::error::CordialError::syn_parse(path.display().to_string(), err)
            })?;
            local_trait_names.extend(collect_local_trait_names(&syntax));
            collect_constructions(&syntax, &mut const_constructed, &mut runtime_constructed);
            parsed.push((syntax, path, tree_root.clone()));
        }
    }
    let const_placed_types =
        types_only_constructed_in_const(&const_constructed, &runtime_constructed);

    let mut findings = Vec::new();
    for (syntax, path, tree_root) in parsed {
        findings.extend(scan_parsed(
            syntax,
            &path,
            &tree_root,
            crate_root,
            &local_trait_names,
            &const_placed_types,
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
    let mut const_constructed = HashSet::new();
    let mut runtime_constructed = HashSet::new();
    collect_constructions(&syntax, &mut const_constructed, &mut runtime_constructed);
    let const_placed_types =
        types_only_constructed_in_const(&const_constructed, &runtime_constructed);
    Ok(scan_parsed(
        syntax,
        file,
        src_root,
        crate_root,
        &local_trait_names,
        &const_placed_types,
    ))
}

#[instrument(
    level = "debug",
    skip(syntax, local_trait_names, const_placed_types, file)
)]
fn scan_parsed(
    syntax: syn::File,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
    local_trait_names: &HashSet<String>,
    const_placed_types: &HashSet<String>,
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
        const_placed_types,
        cfg_sibling_real_params: HashMap::new(),
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
