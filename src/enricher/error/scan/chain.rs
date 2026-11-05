//! Remaining compatibility collection for error-chain and architecture facts.

use super::ErrorIrScanReport;
use crate::error::CordialResult;
use crate::etiquettes::{ErrorIrScanLayers, scan_rust_file_syntax};
use crate::ir::CrateKind;
use crate::loader::{SourceLoadView, path_has_fixtures, quality_scan_trees};
use std::path::Path;
use tracing::instrument;

#[instrument(level = "debug", skip(source, field_strategy))]
pub(in crate::enricher::error) fn scan_crate_error_ir_with_source(
    crate_root: &Path,
    crate_name: &str,
    source: &SourceLoadView,
    kind_fanout_max_call_sites: usize,
    field_strategy: crate::config::ErrorFieldStrategy,
) -> CordialResult<ErrorIrScanReport> {
    scan_crate_error_ir_impl(
        crate_root,
        crate_name,
        source,
        kind_fanout_max_call_sites,
        field_strategy,
        true,
    )
}

#[instrument(level = "debug", skip(source, field_strategy))]
pub(super) fn scan_crate_error_ir_impl(
    crate_root: &Path,
    crate_name: &str,
    source: &SourceLoadView,
    kind_fanout_max_call_sites: usize,
    field_strategy: crate::config::ErrorFieldStrategy,
    relative_paths: bool,
) -> CordialResult<ErrorIrScanReport> {
    let src_root = crate_root.join("src");
    let is_proc_macro_crate = source.crate_kinds().contains(&CrateKind::ProcMacro);
    let mut report = ErrorIrScanReport::default();

    let mut signature_chain =
        crate::etiquettes::internal_error_chain::SignatureChainIndex::default();

    for tree_root in quality_scan_trees(crate_root) {
        if !tree_root.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&tree_root)
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

            let under_src = path.starts_with(&src_root);
            let unit = source.parsed_file(path, "error-ir-scan")?;
            if under_src {
                signature_chain.collect(unit.syntax(), path, &src_root);
            }
            let file_scan = crate::loader::source_walk(path, "error-ir-scan", || {
                scan_rust_file_syntax(
                    unit.syntax(),
                    path,
                    &tree_root,
                    crate_root,
                    crate_name,
                    ErrorIrScanLayers::for_unified_file(under_src, is_proc_macro_crate),
                )
            })?;

            {
                report
                    .compliance
                    .extend(file_scan.compliance().iter().cloned());
            }
        }
    }

    {
        report.compliance.extend(
            crate::etiquettes::internal_error_chain::scan_crate_error_architecture_with_source(
                crate_root,
                crate_name,
                source,
                kind_fanout_max_call_sites,
                field_strategy,
            )?,
        );
        let (findings, mut checks) = signature_chain.assess(crate_name)?;
        report.compliance.extend(findings);
        if relative_paths {
            for check in &mut checks {
                check.strip_file_prefix(crate_root);
            }
        }
        report.signature_checks = checks;
        super::compliance_sort::sort_compliance(&mut report.compliance);
        if relative_paths {
            for finding in &mut report.compliance {
                finding.strip_file_prefix(crate_root);
            }
        }
    }

    Ok(report)
}
