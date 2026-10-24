//! Materialization and typed reads of conversion evidence in the shared IR.

use tracing::instrument;

use crate::error::CordialResult;
use crate::ir::{EdgeKind, IrMut, IrView, NodeKind, NodeWeight, QueryBuilder};
use crate::loader::SharedParsedFile;
use crate::objects::FileSpan;

use super::ErrorConversionUnit;
use super::collection::collect_conversion_units;

const CONVERSION_UNIT_ATTR: &str = "error_conversion_unit";

/// Collect conversion evidence before apply and materialize it in the shared IR.
#[instrument(level = "debug", skip(ir, files), err(level = "warn"))]
pub(crate) fn enrich_conversion_units(
    ir: &mut dyn IrMut,
    files: &[SharedParsedFile],
) -> CordialResult<()> {
    let root = ir.root()?;
    for (unit, line) in collect_conversion_units(files) {
        let node = ir.insert_node(
            NodeWeight::new(NodeKind::Field)
                .with_name(unit.field_name.clone())
                .with_span(FileSpan::new(&unit.declaration, line, 1)),
        )?;
        ir.set_attr(node, CONVERSION_UNIT_ATTR, serde_json::to_value(unit)?)?;
        ir.insert_edge(root, node, EdgeKind::Contains)?;
    }
    Ok(())
}

/// Read conversion evidence from the graph, rejecting malformed facts.
#[instrument(level = "debug", skip(ir), err(level = "warn"))]
pub(crate) fn conversion_units_from_ir(ir: &dyn IrView) -> CordialResult<Vec<ErrorConversionUnit>> {
    let query = QueryBuilder::new().has_attr(CONVERSION_UNIT_ATTR).build();
    ir.nodes_matching(&query)
        .into_iter()
        .map(|node| {
            let value = node
                .attr(CONVERSION_UNIT_ATTR)
                .ok_or_else(|| crate::error::CordialError::unreachable(CONVERSION_UNIT_ATTR))?;
            Ok(serde_json::from_value(value.clone())?)
        })
        .collect()
}
