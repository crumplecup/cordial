use crate::error::CordialResult;

use super::{InternalErrorChainCrateSummary, InternalErrorChainScanReport};

use tracing::instrument;
/// Workspace rollup for internal error-chain scans.
#[derive(Debug, Clone, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct WorkspaceInternalErrorChainSummary {
    /// How many error types were inventoried.
    #[getter(copy)]
    type_nodes: usize,
    /// Internal error types that do not wrap another type.
    #[getter(copy)]
    internal_leaves: usize,
    /// Internal error types that wrap another internal type.
    #[getter(copy)]
    internal_links: usize,
    /// Internal error types that wrap a foreign error.
    #[getter(copy)]
    foreign_bridges: usize,
    /// Compliance findings for this crate.
    #[getter(copy)]
    compliance_findings: usize,
    /// Sites that stringify a typed error.
    #[getter(copy)]
    stringify_violations: usize,
    /// Sites that discard a typed error.
    #[getter(copy)]
    discard_violations: usize,
    /// Crate names in this rollup.
    crates: Vec<InternalErrorChainCrateSummary>,
}

impl WorkspaceInternalErrorChainSummary {
    /// Start a builder for this value.
    pub fn builder() -> WorkspaceInternalErrorChainSummaryBuilder {
        WorkspaceInternalErrorChainSummaryBuilder::default()
    }
}

/// Build workspace internal error chain summary.
#[instrument(level = "debug", skip(reports))]
pub fn build_workspace_internal_error_chain_summary(
    reports: &[InternalErrorChainScanReport],
) -> CordialResult<WorkspaceInternalErrorChainSummary> {
    let mut crates = Vec::with_capacity(reports.len());
    let mut type_nodes = 0usize;
    let mut internal_leaves = 0usize;
    let mut internal_links = 0usize;
    let mut foreign_bridges = 0usize;
    let mut compliance_findings = 0usize;
    let mut stringify_violations = 0usize;
    let mut discard_violations = 0usize;

    for report in reports {
        let counts = report.type_graph().class_counts();
        type_nodes += report.type_graph().nodes().len();
        internal_leaves += counts.internal_leaf;
        internal_links += counts.internal_link + counts.umbrella_wrapper;
        foreign_bridges += counts.foreign_bridge;
        compliance_findings += report.compliance().findings().len();
        stringify_violations += report.compliance().stringify_count();
        discard_violations += report.compliance().discard_count();
        crates.push(
            InternalErrorChainCrateSummary::builder()
                .crate_name(report.crate_name().clone())
                .type_nodes(report.type_graph().nodes().len())
                .internal_leaves(counts.internal_leaf)
                .internal_links(counts.internal_link + counts.umbrella_wrapper)
                .foreign_bridges(counts.foreign_bridge)
                .compliance_findings(report.compliance().findings().len())
                .stringify_violations(report.compliance().stringify_count())
                .discard_violations(report.compliance().discard_count())
                .build()?,
        );
    }

    crates.sort_by(|a, b| a.crate_name().cmp(b.crate_name()));

    WorkspaceInternalErrorChainSummary::builder()
        .type_nodes(type_nodes)
        .internal_leaves(internal_leaves)
        .internal_links(internal_links)
        .foreign_bridges(foreign_bridges)
        .compliance_findings(compliance_findings)
        .stringify_violations(stringify_violations)
        .discard_violations(discard_violations)
        .crates(crates)
        .build()
}
