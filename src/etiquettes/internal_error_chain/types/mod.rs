//! Internal error-chain domain types — one file per type so the tree is the catalog.

mod compliance_finding;
mod compliance_id;
mod compliance_report;
mod crate_summary;
mod finding;
mod marker;
mod node_class;
mod node_class_counts;
mod record_kind;
mod rule;
mod scan_report;
mod type_graph_report;
mod type_node;
mod type_probe_id;
mod workspace_summary;

pub use compliance_finding::InternalErrorComplianceFinding;
pub use compliance_id::InternalErrorComplianceId;
pub use compliance_report::InternalErrorComplianceReport;
pub use crate_summary::InternalErrorChainCrateSummary;
pub use finding::InternalErrorChainFinding;
pub use marker::InternalErrorChainMarker;
pub use node_class::InternalErrorNodeClass;
pub use node_class_counts::InternalErrorNodeClassCounts;
pub use record_kind::InternalErrorRecordKind;
pub use rule::InternalErrorChainRule;
pub use scan_report::InternalErrorChainScanReport;
pub use type_graph_report::InternalErrorTypeGraphReport;
pub use type_node::InternalErrorTypeNode;
pub use type_probe_id::InternalErrorTypeProbeId;
pub use workspace_summary::{
    WorkspaceInternalErrorChainSummary, build_workspace_internal_error_chain_summary,
};
