//! Remaining compatibility collection for error-chain and architecture facts.

use super::ErrorIrScanReport;
use crate::error::CordialResult;
use crate::loader::SourceLoadView;
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
    let mut report = ErrorIrScanReport::default();

    let mut signature_chain =
        crate::etiquettes::internal_error_chain::SignatureChainIndex::default();

    if src_root.is_dir() {
        for entry in walkdir::WalkDir::new(&src_root).sort_by_file_name() {
            let entry = entry.map_err(anyhow::Error::new)?;
            let path = entry.path();
            if entry.file_type().is_file()
                && path.extension().is_some_and(|ext| ext == "rs")
                && !crate::loader::path_has_fixtures(path, crate_root)
            {
                let file = source.parsed_file(path, "error-signature-chain")?;
                signature_chain.collect(file.syntax(), path, &src_root);
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
