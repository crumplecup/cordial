//! Standalone absolute-path reports and stable compliance ordering.

use std::path::Path;

use crate::error::CordialResult;
use crate::etiquettes::internal_error_chain::InternalErrorComplianceFinding;
use crate::loader::SourceLoadView;
use tracing::instrument;

use super::ErrorIrScanReport;
use super::chain::scan_crate_error_ir_impl;

impl ErrorIrScanReport {
    /// Internal report.
    #[instrument(level = "trace", skip(self))]
    pub fn internal_report(
        &self,
        crate_name: &str,
    ) -> crate::etiquettes::internal_error_chain::InternalErrorChainScanReport {
        crate::etiquettes::internal_error_chain::InternalErrorChainScanReport::new(
            crate_name.to_string(),
            self.type_graph.clone(),
            crate::etiquettes::internal_error_chain::InternalErrorComplianceReport::new(
                crate_name.to_string(),
                self.compliance.clone(),
            ),
            self.signature_checks.clone(),
        )
    }
}

#[instrument(level = "debug")]
pub(crate) fn scan_crate_error_ir_with_absolute_paths(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<ErrorIrScanReport> {
    let source = SourceLoadView::for_standalone_scan();
    let sites = crate::ir::SourceErrorSites::collect(&source, crate_root)?.records();
    let mut report = scan_crate_error_ir_impl(
        crate_root,
        crate_name,
        &source,
        crate::config::InternalErrorChainThresholds::default().kind_fanout_max_call_sites(),
        crate::config::ErrorFieldStrategy::default(),
        false,
    )?;
    report.type_graph =
        crate::ir::SourceErrorTypes::collect(&source, crate_root)?.report(crate_name, false)?;
    report.compliance.extend(
        crate::ir::SourceErrorCompliance::collect(&source, crate_root, crate_name)?.findings(false),
    );
    let signatures = crate::ir::SourceErrorSignatures::collect(&source, crate_root)?
        .assess(crate_name, false)?;
    report
        .compliance
        .extend(signatures.findings().iter().cloned());
    report.signature_checks = signatures.checks().clone();
    sort_compliance(&mut report.compliance);
    report.sites = sites;
    report.chain = crate::ir::SourceErrorChains::collect(&source, crate_root)?.records();
    Ok(report)
}

#[instrument(level = "debug", skip(findings))]
pub(super) fn sort_compliance(findings: &mut [InternalErrorComplianceFinding]) {
    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.rule_id().to_string().cmp(&b.rule_id().to_string()))
    });
}
