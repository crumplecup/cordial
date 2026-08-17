use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::{
    Disposition, FileSpan, Finding, FindingSink, IrAnchor, Marker, Rule, SourceSpan,
};

/// Classification of one node in the crate error type graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InternalErrorNodeClass {
    InternalLeaf,
    InternalLink,
    ForeignBridge,
    UmbrellaWrapper,
}

impl InternalErrorNodeClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InternalLeaf => "ERROR-CHAIN-INTERNAL-LEAF",
            Self::InternalLink => "ERROR-CHAIN-INTERNAL-LINK",
            Self::ForeignBridge => "ERROR-CHAIN-FOREIGN-BRIDGE",
            Self::UmbrellaWrapper => "ERROR-CHAIN-INTERNAL-UMBRELLA",
        }
    }

    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ERROR-CHAIN-INTERNAL-LEAF" => Some(Self::InternalLeaf),
            "ERROR-CHAIN-INTERNAL-LINK" => Some(Self::InternalLink),
            "ERROR-CHAIN-FOREIGN-BRIDGE" => Some(Self::ForeignBridge),
            "ERROR-CHAIN-INTERNAL-UMBRELLA" => Some(Self::UmbrellaWrapper),
            _ => None,
        }
    }
}

impl Display for InternalErrorNodeClass {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

/// Type-graph probe rule identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InternalErrorTypeProbeId {
    InternalLeaf001,
    InternalLink001,
    InternalNested001,
}

impl InternalErrorTypeProbeId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InternalLeaf001 => "ERROR-CHAIN-INTERNAL-LEAF-001",
            Self::InternalLink001 => "ERROR-CHAIN-INTERNAL-LINK-001",
            Self::InternalNested001 => "ERROR-CHAIN-INTERNAL-NESTED-001",
        }
    }

    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ERROR-CHAIN-INTERNAL-LEAF-001" => Some(Self::InternalLeaf001),
            "ERROR-CHAIN-INTERNAL-LINK-001" => Some(Self::InternalLink001),
            "ERROR-CHAIN-INTERNAL-NESTED-001" => Some(Self::InternalNested001),
            _ => None,
        }
    }
}

impl Display for InternalErrorTypeProbeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

/// Non-compliant error-handling pattern at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InternalErrorComplianceId {
    StringifyForeign001,
    DiscardTyped001,
}

impl InternalErrorComplianceId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StringifyForeign001 => "ERROR-CHAIN-COMPLIANCE-STRINGIFY-001",
            Self::DiscardTyped001 => "ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001",
        }
    }

    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "ERROR-CHAIN-COMPLIANCE-STRINGIFY-001" => Some(Self::StringifyForeign001),
            "ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001" => Some(Self::DiscardTyped001),
            _ => None,
        }
    }
}

impl Display for InternalErrorComplianceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}

/// Distinguishes type-graph inventory rows from compliance violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalErrorRecordKind {
    TypeGraph,
    Compliance,
}

impl InternalErrorRecordKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TypeGraph => "type_graph",
            Self::Compliance => "compliance",
        }
    }

    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "type_graph" => Some(Self::TypeGraph),
            "compliance" => Some(Self::Compliance),
            _ => None,
        }
    }
}

/// One row in the static error type graph inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalErrorTypeNode {
    pub crate_name: String,
    pub type_path: String,
    pub node_class: InternalErrorNodeClass,
    pub probe_id: InternalErrorTypeProbeId,
    pub source_target: Option<String>,
    pub reaches_foreign: bool,
    pub chain_depth: u32,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
}

/// One non-compliant error-handling site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalErrorComplianceFinding {
    pub crate_name: String,
    pub rule_id: InternalErrorComplianceId,
    pub context: String,
    pub file: PathBuf,
    pub line: u32,
    pub snippet: String,
    pub foreign_error_type: Option<String>,
    pub internal_constructor: Option<String>,
}

/// Type graph scan output for one crate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InternalErrorTypeGraphReport {
    pub crate_name: String,
    pub nodes: Vec<InternalErrorTypeNode>,
}

/// Compliance scan output for one crate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InternalErrorComplianceReport {
    pub crate_name: String,
    pub findings: Vec<InternalErrorComplianceFinding>,
}

/// Combined internal error-chain scan for one crate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InternalErrorChainScanReport {
    pub crate_name: String,
    pub type_graph: InternalErrorTypeGraphReport,
    pub compliance: InternalErrorComplianceReport,
}

/// Count type-graph nodes by class.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InternalErrorNodeClassCounts {
    pub internal_leaf: usize,
    pub internal_link: usize,
    pub foreign_bridge: usize,
    pub umbrella_wrapper: usize,
}

impl InternalErrorTypeGraphReport {
    pub fn class_counts(&self) -> InternalErrorNodeClassCounts {
        let mut counts = InternalErrorNodeClassCounts::default();
        for node in &self.nodes {
            match node.node_class {
                InternalErrorNodeClass::InternalLeaf => counts.internal_leaf += 1,
                InternalErrorNodeClass::InternalLink => counts.internal_link += 1,
                InternalErrorNodeClass::ForeignBridge => counts.foreign_bridge += 1,
                InternalErrorNodeClass::UmbrellaWrapper => counts.umbrella_wrapper += 1,
            }
        }
        counts
    }
}

impl InternalErrorComplianceReport {
    pub fn stringify_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.rule_id == InternalErrorComplianceId::StringifyForeign001)
            .count()
    }

    pub fn discard_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.rule_id == InternalErrorComplianceId::DiscardTyped001)
            .count()
    }
}

/// Per-crate rollup row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalErrorChainCrateSummary {
    pub crate_name: String,
    pub type_nodes: usize,
    pub internal_leaves: usize,
    pub internal_links: usize,
    pub foreign_bridges: usize,
    pub compliance_findings: usize,
    pub stringify_violations: usize,
    pub discard_violations: usize,
}

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

#[derive(Debug, Clone)]
pub struct InternalErrorChainRule {
    pub rule_id: String,
}

impl InternalErrorChainRule {
    pub fn from_probe(probe_id: InternalErrorTypeProbeId) -> Self {
        Self {
            rule_id: probe_id.as_str().to_string(),
        }
    }

    pub fn from_compliance(compliance_id: InternalErrorComplianceId) -> Self {
        Self {
            rule_id: compliance_id.as_str().to_string(),
        }
    }
}

impl Rule for InternalErrorChainRule {
    fn id(&self) -> &str {
        &self.rule_id
    }

    fn category(&self) -> &str {
        "internal_error_chain"
    }

    fn description(&self) -> &str {
        "Internal error type graph node or compliance violation"
    }
}

#[derive(Debug, Clone)]
pub struct InternalErrorChainMarker {
    pub anchor: crate::objects::NodeAnchor,
}

impl Marker for InternalErrorChainMarker {
    fn probe(&self) -> &str {
        "internal-error-chain"
    }

    fn label(&self) -> &str {
        "internal-error-chain"
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct InternalErrorChainFinding {
    pub rule: InternalErrorChainRule,
    pub record_kind: InternalErrorRecordKind,
    pub disposition: Disposition,
    pub anchor: crate::objects::NodeAnchor,
    pub crate_name: String,
    pub context: String,
    pub span: FileSpan,
    pub snippet: String,
    pub type_path: Option<String>,
    pub node_class: Option<InternalErrorNodeClass>,
    pub source_target: Option<String>,
    pub reaches_foreign: Option<bool>,
    pub chain_depth: Option<u32>,
    pub foreign_error_type: Option<String>,
    pub internal_constructor: Option<String>,
}

impl Finding for InternalErrorChainFinding {
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    fn disposition(&self) -> Disposition {
        self.disposition
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn emit(&self, sink: &mut dyn FindingSink) {
        sink.field("crate", &self.crate_name);
        sink.field("record_kind", &self.record_kind.as_str());
        sink.field("rule_id", &self.rule.rule_id);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("snippet", &self.snippet);
        if let Some(type_path) = &self.type_path {
            sink.field("type_path", type_path);
        }
        if let Some(node_class) = self.node_class {
            sink.field("node_class", &node_class.to_string());
        }
        sink.field(
            "source_target",
            &self.source_target.clone().unwrap_or_default(),
        );
        if let Some(reaches_foreign) = self.reaches_foreign {
            sink.field("reaches_foreign", &reaches_foreign.to_string());
        }
        if let Some(chain_depth) = self.chain_depth {
            sink.field("chain_depth", &chain_depth.to_string());
        }
        sink.field(
            "foreign_error_type",
            &self.foreign_error_type.clone().unwrap_or_default(),
        );
        sink.field(
            "internal_constructor",
            &self.internal_constructor.clone().unwrap_or_default(),
        );
        sink.snippet(&self.snippet);
    }
}
