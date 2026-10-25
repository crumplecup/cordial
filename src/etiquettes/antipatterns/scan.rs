//! syn-based scan for antipattern probes (`Box<dyn Error>`, `Result<_, String>`, `&'static` struct fields except crate-local `dyn Trait` and const/static-only tables, …).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use syn::visit::Visit;

use crate::config::{ErrorFieldStrategy, StaticRefStrategy};
use crate::error::CordialResult;
use crate::loader::{
    SharedParsedFile, SourceLoadView, module_path_from_src_file, path_has_fixtures,
    quality_scan_trees,
};

use super::types::AntipatternSiteRecord;

use tracing::instrument;
mod constructions;
mod preds;
mod visitor;

use constructions::{collect_constructions, types_only_constructed_in_const};
use visitor::{
    AntipatternScanVisitor, collect_error_kind_names, collect_error_type_names,
    collect_local_trait_names,
};

pub(crate) use preds::truncate_snippet;

#[instrument(
    level = "debug",
    skip(source, static_ref_strategy, error_field_strategy, skip_paths),
    err(level = "warn")
)]
pub(super) fn scan_shared_crate_trees(
    source: &SourceLoadView,
    crate_root: &Path,
    static_ref_strategy: StaticRefStrategy,
    error_field_strategy: ErrorFieldStrategy,
    skip_paths: &HashMap<String, Vec<String>>,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let mut parsed: Vec<(SharedParsedFile, PathBuf)> = Vec::new();
    let mut local_trait_names = HashSet::new();
    let mut error_type_names = HashSet::new();
    let mut const_constructed = HashSet::new();
    let mut runtime_constructed = HashSet::new();
    for tree_root in quality_scan_trees(crate_root) {
        for file in source.parsed_rust_files_under(&tree_root, "antipattern-inventory", |path| {
            !path_has_fixtures(path, crate_root)
        })? {
            local_trait_names.extend(collect_local_trait_names(file.syntax()));
            error_type_names.extend(collect_error_type_names(file.syntax()));
            collect_constructions(
                file.syntax(),
                &mut const_constructed,
                &mut runtime_constructed,
            );
            parsed.push((file, tree_root.clone()));
        }
    }
    let const_placed_types =
        types_only_constructed_in_const(&const_constructed, &runtime_constructed);
    for (file, _) in &parsed {
        let kinds = collect_error_kind_names(file.syntax(), &error_type_names);
        error_type_names.extend(kinds);
    }

    let mut findings = Vec::new();
    for (file, tree_root) in parsed {
        findings.extend(scan_shared_file(
            &file,
            &tree_root,
            crate_root,
            &local_trait_names,
            &error_type_names,
            &const_placed_types,
            static_ref_strategy,
            error_field_strategy,
            skip_paths,
        )?);
    }
    Ok(findings)
}

#[instrument(
    level = "debug",
    skip(
        file,
        local_trait_names,
        error_type_names,
        const_placed_types,
        static_ref_strategy,
        error_field_strategy,
        skip_paths
    ),
    err(level = "warn")
)]
fn scan_shared_file(
    file: &SharedParsedFile,
    tree_root: &Path,
    crate_root: &Path,
    local_trait_names: &HashSet<String>,
    error_type_names: &HashSet<String>,
    const_placed_types: &HashSet<String>,
    static_ref_strategy: StaticRefStrategy,
    error_field_strategy: ErrorFieldStrategy,
    skip_paths: &HashMap<String, Vec<String>>,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    scan_parsed(
        file.syntax(),
        file.path(),
        tree_root,
        crate_root,
        local_trait_names,
        error_type_names,
        const_placed_types,
        static_ref_strategy,
        error_field_strategy,
        skip_paths,
    )
}

/// Scan one Rust source file and return records.
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    scan_rust_source_with_static_ref_strategy(
        source,
        file,
        src_root,
        crate_root,
        StaticRefStrategy::default(),
    )
}

/// Scan one Rust source file and return records using a static-reference strategy.
#[instrument(level = "debug", skip(source, file, static_ref_strategy))]
pub fn scan_rust_source_with_static_ref_strategy(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
    static_ref_strategy: StaticRefStrategy,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    scan_rust_source_with_strategies(
        source,
        file,
        src_root,
        crate_root,
        static_ref_strategy,
        ErrorFieldStrategy::Owned,
    )
}

/// Scan one source file with general static-reference and error-field policies.
#[instrument(
    level = "debug",
    skip(source, file, static_ref_strategy, error_field_strategy)
)]
pub fn scan_rust_source_with_strategies(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
    static_ref_strategy: StaticRefStrategy,
    error_field_strategy: ErrorFieldStrategy,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    scan_rust_source_with_skip_paths(
        source,
        file,
        src_root,
        crate_root,
        static_ref_strategy,
        error_field_strategy,
        &HashMap::new(),
    )
}

/// Like [`scan_rust_source_with_strategies`], but `skip_paths` is
/// `[antipatterns] skip_paths`: rule id to crate-relative path prefixes
/// exempt from that rule.
#[instrument(
    level = "debug",
    skip(source, file, static_ref_strategy, error_field_strategy, skip_paths)
)]
pub fn scan_rust_source_with_skip_paths(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
    static_ref_strategy: StaticRefStrategy,
    error_field_strategy: ErrorFieldStrategy,
    skip_paths: &HashMap<String, Vec<String>>,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    let local_trait_names = collect_local_trait_names(&syntax);
    let mut error_type_names = collect_error_type_names(&syntax);
    let kinds = collect_error_kind_names(&syntax, &error_type_names);
    error_type_names.extend(kinds);
    let mut const_constructed = HashSet::new();
    let mut runtime_constructed = HashSet::new();
    collect_constructions(&syntax, &mut const_constructed, &mut runtime_constructed);
    let const_placed_types =
        types_only_constructed_in_const(&const_constructed, &runtime_constructed);
    scan_parsed(
        &syntax,
        file,
        src_root,
        crate_root,
        &local_trait_names,
        &error_type_names,
        &const_placed_types,
        static_ref_strategy,
        error_field_strategy,
        skip_paths,
    )
}

#[instrument(
    level = "debug",
    skip(
        syntax,
        file,
        local_trait_names,
        error_type_names,
        const_placed_types,
        static_ref_strategy,
        error_field_strategy,
        skip_paths
    )
)]
fn scan_parsed(
    syntax: &syn::File,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
    local_trait_names: &HashSet<String>,
    error_type_names: &HashSet<String>,
    const_placed_types: &HashSet<String>,
    static_ref_strategy: StaticRefStrategy,
    error_field_strategy: ErrorFieldStrategy,
    skip_paths: &HashMap<String, Vec<String>>,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = AntipatternScanVisitor::new(
        file.to_path_buf(),
        crate_root.to_path_buf(),
        module_prefix,
        local_trait_names,
        error_type_names,
        const_placed_types,
        static_ref_strategy,
        error_field_strategy,
        skip_paths,
    );
    visitor.visit_file(syntax);
    visitor.into_findings()
}
