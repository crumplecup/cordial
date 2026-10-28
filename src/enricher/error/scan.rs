//! Unified error IR source scan — one `syn` parse per file, one AST walk per layer group.

use std::path::Path;

use crate::error::CordialResult;
use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::etiquettes::{ErrorIrScanLayers, scan_rust_file_syntax};
use crate::ir::CrateKind;
use crate::loader::{SourceLoadView, path_has_fixtures, quality_scan_trees};

#[cfg(feature = "error_chain")]
use crate::etiquettes::error_chain::ErrorChainRecord;
use tracing::instrument;

/// Combined scan output for one crate (one parse + unified walk per file).
#[derive(Debug, Clone, Default, derive_getters::Getters)]
pub struct ErrorIrScanReport {
    /// Error-site records from this crate.
    sites: Vec<ErrorSiteRecord>,
    /// Error-chain records from this crate.
    #[cfg(feature = "error_chain")]
    chain: Vec<ErrorChainRecord>,
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

impl ErrorIrScanReport {
    /// Internal report.
    #[instrument(level = "trace", skip(self))]
    #[cfg(feature = "internal_error_chain")]
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

/// Scan all quality source trees once per file for error-handling IR facts.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_ir(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<ErrorIrScanReport> {
    let source = SourceLoadView::for_standalone_scan();
    #[cfg(feature = "internal_error_chain")]
    let kind_fanout_max_call_sites =
        crate::config::InternalErrorChainThresholds::default().kind_fanout_max_call_sites();
    #[cfg(not(feature = "internal_error_chain"))]
    let kind_fanout_max_call_sites = 0;
    scan_crate_error_ir_impl(
        crate_root,
        crate_name,
        &source,
        kind_fanout_max_call_sites,
        crate::config::ErrorFieldStrategy::default(),
        true,
    )
}

#[instrument(level = "debug", skip(source, field_strategy))]
pub(super) fn scan_crate_error_ir_with_source(
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
fn scan_crate_error_ir_impl(
    crate_root: &Path,
    crate_name: &str,
    source: &SourceLoadView,
    kind_fanout_max_call_sites: usize,
    field_strategy: crate::config::ErrorFieldStrategy,
    relative_paths: bool,
) -> CordialResult<ErrorIrScanReport> {
    #[cfg(not(feature = "internal_error_chain"))]
    let _ = (field_strategy, kind_fanout_max_call_sites);
    let src_root = crate_root.join("src");
    let is_proc_macro_crate = source.crate_kinds().contains(&CrateKind::ProcMacro);
    let mut report = ErrorIrScanReport::default();

    #[cfg(feature = "internal_error_chain")]
    let mut type_graph_raw = Vec::<crate::etiquettes::internal_error_chain::RawTypeNode>::new();
    #[cfg(feature = "internal_error_chain")]
    let mut error_impls = std::collections::BTreeSet::<String>::new();
    #[cfg(feature = "internal_error_chain")]
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
            #[cfg(feature = "internal_error_chain")]
            if under_src {
                signature_chain.collect(unit.syntax(), path, &src_root);
            }
            let file_scan = crate::loader::source_walk(path, "error-ir-scan", || {
                scan_rust_file_syntax(
                    unit.syntax(),
                    path,
                    &tree_root,
                    &src_root,
                    crate_root,
                    crate_name,
                    ErrorIrScanLayers::for_unified_file(under_src, is_proc_macro_crate),
                )
            })?;

            report.sites.extend(file_scan.sites().iter().cloned());
            #[cfg(feature = "error_chain")]
            report.chain.extend(file_scan.chain().iter().cloned());
            #[cfg(feature = "internal_error_chain")]
            {
                report
                    .compliance
                    .extend(file_scan.compliance().iter().cloned());
                type_graph_raw.extend(file_scan.type_graph_raw().iter().cloned());
                error_impls.extend(file_scan.error_impls().iter().cloned());
            }
        }
    }

    #[cfg(feature = "internal_error_chain")]
    {
        type_graph_raw.retain(|node| {
            crate::etiquettes::internal_error_chain::type_path_is_error_related(
                node.type_path(),
                &error_impls,
            )
        });
        let mut nodes = crate::etiquettes::internal_error_chain::finalize_type_graph(
            type_graph_raw,
            crate_name,
        )?;
        if relative_paths {
            for node in &mut nodes {
                node.strip_file_prefix(crate_root);
            }
        }
        report.type_graph =
            crate::etiquettes::internal_error_chain::InternalErrorTypeGraphReport::new(
                crate_name.to_string(),
                nodes,
            );
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
        compliance_sort::sort_compliance(&mut report.compliance);
        if relative_paths {
            for finding in &mut report.compliance {
                finding.strip_file_prefix(crate_root);
            }
        }
    }

    sort_sites(&mut report.sites);
    #[cfg(feature = "error_chain")]
    chain_sort::sort_chain(&mut report.chain);

    Ok(report)
}

#[instrument(level = "debug", skip(sites))]
fn sort_sites(sites: &mut [ErrorSiteRecord]) {
    sites.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.kind().to_string().cmp(&b.kind().to_string()))
            .then(a.source_snippet().cmp(b.source_snippet()))
    });
}

#[cfg(feature = "error_chain")]
mod chain_sort {
    use crate::etiquettes::error_chain::ErrorChainRecord;
    use tracing::instrument;

    #[instrument(level = "debug", skip(chain))]
    pub(super) fn sort_chain(chain: &mut [ErrorChainRecord]) {
        chain.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.rule_id().to_string().cmp(&b.rule_id().to_string()))
        });
    }
}

#[cfg(feature = "internal_error_chain")]
pub(super) mod compliance_sort {
    use std::path::Path;

    use crate::error::CordialResult;
    use crate::etiquettes::internal_error_chain::InternalErrorComplianceFinding;
    use crate::loader::SourceLoadView;
    use tracing::instrument;

    use super::{ErrorIrScanReport, scan_crate_error_ir_impl};

    #[instrument(level = "debug")]
    pub(crate) fn scan_crate_error_ir_with_absolute_paths(
        crate_root: &Path,
        crate_name: &str,
    ) -> CordialResult<ErrorIrScanReport> {
        let source = SourceLoadView::for_standalone_scan();
        scan_crate_error_ir_impl(
            crate_root,
            crate_name,
            &source,
            crate::config::InternalErrorChainThresholds::default().kind_fanout_max_call_sites(),
            crate::config::ErrorFieldStrategy::default(),
            false,
        )
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
