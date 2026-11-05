//! Materialize loader-owned compliance without source access.

use crate::CordialResult;
use crate::enricher::resolve_parent;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{EdgeKind, IrMut, NodeKind, NodeWeight};
use crate::objects::FileSpan;
use tracing::instrument;

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ErrorComplianceInventoryEnricher;

pub(super) static ERROR_COMPLIANCE_INVENTORY: ErrorComplianceInventoryEnricher =
    ErrorComplianceInventoryEnricher;

impl ErrorComplianceInventoryEnricher {
    pub const ID: &'static str = "error-compliance-inventory";
}

impl IrEnricher for ErrorComplianceInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        50
    }

    #[instrument(level = "trace", skip(self, view), err(level = "warn"))]
    #[hotpath::measure(impl_type = "ErrorComplianceInventoryEnricher")]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir();
        let inventory = ir.source_error_compliance()?;
        materialize(ir, inventory.crate_root(), &inventory.findings(true))
    }
}

#[instrument(level = "debug", skip(ir, findings), err(level = "warn"))]
pub(super) fn materialize(
    ir: &mut dyn IrMut,
    crate_root: &std::path::Path,
    findings: &[crate::etiquettes::internal_error_chain::InternalErrorComplianceFinding],
) -> CordialResult<()> {
    for finding in findings {
        let parent = resolve_parent(ir, finding.context())?;
        let file = crate_root.join(finding.file());
        let span = FileSpan::new(file.clone(), finding.line(), 1);
        let ir_node = ir.insert_node(
            NodeWeight::new(NodeKind::Expr)
                .with_span(span.clone())
                .with_name(finding.snippet().clone()),
        )?;
        ir.set_attr(
            ir_node,
            "internal_error_record_kind",
            serde_json::Value::String("compliance".to_string()),
        )?;
        ir.set_attr(
            ir_node,
            "rule_id",
            serde_json::Value::String(finding.rule_id().to_string()),
        )?;
        ir.set_attr(
            ir_node,
            "context",
            serde_json::Value::String(finding.context().clone()),
        )?;
        ir.set_attr(
            ir_node,
            "foreign_error_type",
            serde_json::Value::String(finding.foreign_error_type().clone().unwrap_or_default()),
        )?;
        ir.set_attr(
            ir_node,
            "internal_constructor",
            serde_json::Value::String(finding.internal_constructor().clone().unwrap_or_default()),
        )?;
        ir.set_attr(
            ir_node,
            "snippet",
            serde_json::Value::String(finding.snippet().clone()),
        )?;
        ir.set_attr(
            ir_node,
            "file",
            serde_json::Value::String(file.display().to_string()),
        )?;
        ir.set_attr(
            ir_node,
            "line",
            serde_json::Value::Number(finding.line().into()),
        )?;
        ir.insert_edge(parent, ir_node, EdgeKind::Contains)?;
    }
    Ok(())
}
