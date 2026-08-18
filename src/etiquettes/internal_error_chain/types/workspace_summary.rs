use super::{InternalErrorChainCrateSummary, InternalErrorChainScanReport};

use tracing::instrument;
/// Workspace rollup for internal error-chain scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInternalErrorChainSummary {
    pub type_nodes: usize,
    pub internal_leaves: usize,
    pub internal_links: usize,
    pub foreign_bridges: usize,
    pub compliance_findings: usize,
    pub stringify_violations: usize,
    pub discard_violations: usize,
    pub crates: Vec<InternalErrorChainCrateSummary>,
}

#[instrument(level = "debug")]
pub fn build_workspace_internal_error_chain_summary(
    reports: &[InternalErrorChainScanReport],
) -> WorkspaceInternalErrorChainSummary {
    let mut crates = Vec::with_capacity(reports.len());
    let mut type_nodes = 0usize;
    let mut internal_leaves = 0usize;
    let mut internal_links = 0usize;
    let mut foreign_bridges = 0usize;
    let mut compliance_findings = 0usize;
    let mut stringify_violations = 0usize;
    let mut discard_violations = 0usize;

    for report in reports {
        let counts = report.type_graph.class_counts();
        type_nodes += report.type_graph.nodes.len();
        internal_leaves += counts.internal_leaf;
        internal_links += counts.internal_link + counts.umbrella_wrapper;
        foreign_bridges += counts.foreign_bridge;
        compliance_findings += report.compliance.findings.len();
        stringify_violations += report.compliance.stringify_count();
        discard_violations += report.compliance.discard_count();
        crates.push(InternalErrorChainCrateSummary {
            crate_name: report.crate_name.clone(),
            type_nodes: report.type_graph.nodes.len(),
            internal_leaves: counts.internal_leaf,
            internal_links: counts.internal_link + counts.umbrella_wrapper,
            foreign_bridges: counts.foreign_bridge,
            compliance_findings: report.compliance.findings.len(),
            stringify_violations: report.compliance.stringify_count(),
            discard_violations: report.compliance.discard_count(),
        });
    }

    crates.sort_by(|a, b| a.crate_name.cmp(&b.crate_name));

    WorkspaceInternalErrorChainSummary {
        type_nodes,
        internal_leaves,
        internal_links,
        foreign_bridges,
        compliance_findings,
        stringify_violations,
        discard_violations,
        crates,
    }
}
