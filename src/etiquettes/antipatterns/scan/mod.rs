//! syn-based scan for antipattern probes (`Box<dyn Error>`, `Result<_, String>`, `&'static` struct fields, …).

use std::path::Path;

use syn::visit::Visit;

use crate::error::CordialResult;
use crate::loader::{module_path_from_src_file, path_has_fixtures};

use super::types::AntipatternSiteRecord;

use tracing::instrument;
mod preds;
mod visitor;

use visitor::AntipatternScanVisitor;

pub(crate) use preds::truncate_snippet;

#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_source_tree(
    tree_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
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
        if path.extension().is_none_or(|ext| ext != "rs") || path_has_fixtures(path, crate_root) {
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
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = AntipatternScanVisitor {
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        in_trait_definition: false,
        findings: Vec::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.findings)
}
