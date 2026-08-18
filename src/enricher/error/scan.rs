//! Unified error IR source scan — one `syn` parse per file, one AST walk per layer group.

use std::path::Path;

use crate::error::CordialResult;
use crate::etiquettes::error_sites::ErrorSiteRecord;
use crate::etiquettes::{ErrorIrScanLayers, scan_rust_file_syntax};
use crate::loader::{is_error_module_path, path_has_fixtures, quality_scan_trees};

use tracing::instrument;
#[cfg(feature = "error_chain")]
use crate::etiquettes::error_chain::ErrorChainRecord;

#[cfg(feature = "internal_error_chain")]
use crate::etiquettes::internal_error_chain::{
    InternalErrorChainScanReport, InternalErrorComplianceFinding, InternalErrorComplianceReport,
    InternalErrorTypeGraphReport, RawTypeNode, finalize_type_graph,
};

/// Combined scan output for one crate (one parse + unified walk per file).
#[derive(Debug, Clone, Default)]
pub struct ErrorIrScanReport {
    pub sites: Vec<ErrorSiteRecord>,
    #[cfg(feature = "error_chain")]
    pub chain: Vec<ErrorChainRecord>,
    #[cfg(feature = "internal_error_chain")]
    pub type_graph: InternalErrorTypeGraphReport,
    #[cfg(feature = "internal_error_chain")]
    pub compliance: Vec<InternalErrorComplianceFinding>,
}

impl ErrorIrScanReport {
    #[instrument(level = "trace", skip(self))]
    #[cfg(feature = "internal_error_chain")]
    pub fn internal_report(&self, crate_name: &str) -> InternalErrorChainScanReport {
        InternalErrorChainScanReport {
            crate_name: crate_name.to_string(),
            type_graph: self.type_graph.clone(),
            compliance: InternalErrorComplianceReport {
                crate_name: crate_name.to_string(),
                findings: self.compliance.clone(),
            },
        }
    }
}

/// Scan all quality source trees once per file for error-handling IR facts.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_ir(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<ErrorIrScanReport> {
    let src_root = crate_root.join("src");
    let error_root = src_root.join("error");
    let mut report = ErrorIrScanReport::default();

    #[cfg(feature = "internal_error_chain")]
    let mut type_graph_raw = Vec::<RawTypeNode>::new();

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
            let under_error_module = is_error_module_path(path, &src_root);
            let type_graph_root = if path == src_root.join("error.rs") {
                src_root.as_path()
            } else {
                error_root.as_path()
            };
            let file_scan = scan_rust_file_syntax(
                &syntax,
                path,
                &tree_root,
                &src_root,
                type_graph_root,
                crate_root,
                crate_name,
                ErrorIrScanLayers::for_unified_file(under_src, under_error_module),
            );

            report.sites.extend(file_scan.sites);
            #[cfg(feature = "error_chain")]
            report.chain.extend(file_scan.chain);
            #[cfg(feature = "internal_error_chain")]
            {
                report.compliance.extend(file_scan.compliance);
                type_graph_raw.extend(file_scan.type_graph_raw);
            }
        }
    }

    #[cfg(feature = "internal_error_chain")]
    {
        let mut nodes = finalize_type_graph(type_graph_raw, crate_name);
        for node in &mut nodes {
            if let Ok(rel) = node.file.strip_prefix(crate_root) {
                node.file = rel.to_path_buf();
            }
        }
        report.type_graph = InternalErrorTypeGraphReport {
            crate_name: crate_name.to_string(),
            nodes,
        };
        compliance_sort::sort_compliance(&mut report.compliance);
        for finding in &mut report.compliance {
            if let Ok(rel) = finding.file.strip_prefix(crate_root) {
                finding.file = rel.to_path_buf();
            }
        }
    }

    sort_sites(&mut report.sites);
    #[cfg(feature = "error_chain")]
    chain_sort::sort_chain(&mut report.chain);

    Ok(report)
}

fn sort_sites(sites: &mut [ErrorSiteRecord]) {
    sites.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.kind.to_string().cmp(&b.kind.to_string()))
            .then(a.source_snippet.cmp(&b.source_snippet))
    });
}

#[cfg(feature = "error_chain")]
mod chain_sort {
    use super::*;

    pub(super) fn sort_chain(chain: &mut [ErrorChainRecord]) {
        chain.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line.cmp(&b.line))
                .then(a.rule_id.to_string().cmp(&b.rule_id.to_string()))
        });
    }
}

#[cfg(feature = "internal_error_chain")]
mod compliance_sort {
    use super::*;

    pub(super) fn sort_compliance(findings: &mut [InternalErrorComplianceFinding]) {
        findings.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line.cmp(&b.line))
                .then(a.rule_id.to_string().cmp(&b.rule_id.to_string()))
        });
    }
}
