//! Antipattern adapters using the canonical loader syntax and native facts.

use super::types::AntipatternSiteRecord;
use crate::config::{ErrorFieldStrategy, StaticRefStrategy};
use crate::{CordialResult, SourceLoadView};
use std::collections::HashMap;
use std::path::Path;
use tracing::instrument;

#[instrument(level = "debug")]
pub(crate) fn truncate_snippet(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    format!("{truncated}…")
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
#[instrument(
    level = "debug",
    skip(source, file, static_ref_strategy),
    err(level = "warn")
)]
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
    skip(source, file, static_ref_strategy, error_field_strategy),
    err(level = "warn")
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
    skip(source, file, static_ref_strategy, error_field_strategy, skip_paths),
    err(level = "warn")
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
    SourceLoadView::collect_native_fixture(file, source, |source| {
        super::native::collect_file(source, src_root, crate_root)?.assess(
            static_ref_strategy,
            error_field_strategy,
            skip_paths,
        )
    })
}
