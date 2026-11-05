//! Compatibility error-chain collection from the shared source loader.

use std::path::Path;

use crate::error::CordialResult;
use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::loader::SourceLoadView;

use tracing::instrument;

/// Combined scan output for one crate, including loader-collected sites.
#[derive(Debug, Clone, Default, derive_getters::Getters)]
pub struct ErrorIrScanReport {
    /// Error-site records from this crate.
    sites: Vec<ErrorSiteRecord>,
    /// Error-chain records from this crate.
    #[cfg(feature = "error_chain")]
    chain: Vec<crate::etiquettes::error_chain::ErrorChainRecord>,
    /// Type-relationship graph for this crate.
    #[cfg(feature = "internal_error_chain")]
    type_graph: crate::etiquettes::internal_error_chain::InternalErrorTypeGraphReport,
    /// Compliance findings for this crate.
    #[cfg(feature = "internal_error_chain")]
    compliance: Vec<crate::etiquettes::internal_error_chain::InternalErrorComplianceFinding>,
    /// Signature-level source-chain assessments, including unresolved calls.
    #[cfg(feature = "internal_error_chain")]
    signature_checks: Vec<crate::etiquettes::internal_error_chain::SignatureChainCheck>,
}

/// Scan quality source trees using shared parses and loader-collected sites.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_ir(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<ErrorIrScanReport> {
    let source = SourceLoadView::for_standalone_scan();
    let sites = crate::ir::SourceErrorSites::collect(&source, crate_root)?.records();
    #[cfg(feature = "internal_error_chain")]
    let mut report = {
        let kind_fanout_max_call_sites =
            crate::config::InternalErrorChainThresholds::default().kind_fanout_max_call_sites();
        chain::scan_crate_error_ir_impl(
            crate_root,
            crate_name,
            &source,
            kind_fanout_max_call_sites,
            crate::config::ErrorFieldStrategy::default(),
            true,
        )?
    };
    #[cfg(not(feature = "internal_error_chain"))]
    let mut report = {
        let _ = crate_name;
        ErrorIrScanReport::default()
    };
    #[cfg(feature = "error_chain")]
    {
        report.chain = crate::ir::SourceErrorChains::collect(&source, crate_root)?.records();
    }
    #[cfg(feature = "internal_error_chain")]
    {
        report.type_graph =
            crate::ir::SourceErrorTypes::collect(&source, crate_root)?.report(crate_name, true)?;
        report.compliance.extend(
            crate::ir::SourceErrorCompliance::collect(&source, crate_root, crate_name)?
                .findings(true),
        );
        compliance_sort::sort_compliance(&mut report.compliance);
    }
    report.sites = sites;
    Ok(report)
}

#[cfg(feature = "internal_error_chain")]
pub(super) mod chain;

#[cfg(feature = "internal_error_chain")]
pub(super) mod compliance_sort {
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
            crate::ir::SourceErrorCompliance::collect(&source, crate_root, crate_name)?
                .findings(false),
        );
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
}
