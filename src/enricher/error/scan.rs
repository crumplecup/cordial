//! Unified error IR source scan — one `syn` parse per file, one AST walk per layer group.

use std::path::Path;

use crate::error::CordialResult;
use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::etiquettes::{ErrorIrScanLayers, scan_rust_file_syntax};
use crate::loader::{path_has_fixtures, quality_scan_trees};

#[cfg(feature = "error_chain")]
use crate::etiquettes::error_chain::ErrorChainRecord;
use tracing::instrument;

#[cfg(feature = "internal_error_chain")]
use std::collections::BTreeSet;

#[cfg(feature = "internal_error_chain")]
use crate::etiquettes::internal_error_chain::{
    InternalErrorChainScanReport, InternalErrorComplianceFinding, InternalErrorComplianceReport,
    InternalErrorTypeGraphReport, RawTypeNode, finalize_type_graph, scan_crate_error_architecture,
    type_path_is_error_related,
};

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
    type_graph: InternalErrorTypeGraphReport,
    /// Compliance findings for this crate.
    #[cfg(feature = "internal_error_chain")]
    compliance: Vec<InternalErrorComplianceFinding>,
}

impl ErrorIrScanReport {
    /// Internal report.
    #[instrument(level = "trace", skip(self))]
    #[cfg(feature = "internal_error_chain")]
    pub fn internal_report(&self, crate_name: &str) -> InternalErrorChainScanReport {
        InternalErrorChainScanReport::new(
            crate_name.to_string(),
            self.type_graph.clone(),
            InternalErrorComplianceReport::new(crate_name.to_string(), self.compliance.clone()),
        )
    }
}

/// Scan all quality source trees once per file for error-handling IR facts.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_ir(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<ErrorIrScanReport> {
    let src_root = crate_root.join("src");
    let mut report = ErrorIrScanReport::default();

    #[cfg(feature = "internal_error_chain")]
    let mut type_graph_raw = Vec::<RawTypeNode>::new();
    #[cfg(feature = "internal_error_chain")]
    let mut error_impls = BTreeSet::<String>::new();

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

            let source = std::fs::read_to_string(path)?;
            let syntax = syn::parse_file(&source).map_err(|err| {
                crate::error::CordialError::syn_parse(path.display().to_string(), err)
            })?;

            let under_src = path.starts_with(&src_root);
            let file_scan = scan_rust_file_syntax(
                &syntax,
                path,
                &tree_root,
                &src_root,
                crate_root,
                crate_name,
                ErrorIrScanLayers::for_unified_file(under_src),
            )?;

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
        type_graph_raw.retain(|node| type_path_is_error_related(node.type_path(), &error_impls));
        let mut nodes = finalize_type_graph(type_graph_raw, crate_name)?;
        for node in &mut nodes {
            node.strip_file_prefix(crate_root);
        }
        report.type_graph = InternalErrorTypeGraphReport::new(crate_name.to_string(), nodes);
        report
            .compliance
            .extend(scan_crate_error_architecture(crate_root, crate_name)?);
        compliance_sort::sort_compliance(&mut report.compliance);
        for finding in &mut report.compliance {
            finding.strip_file_prefix(crate_root);
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
mod compliance_sort {
    use crate::etiquettes::internal_error_chain::InternalErrorComplianceFinding;
    use tracing::instrument;

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
