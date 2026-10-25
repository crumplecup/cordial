//! Materialization and typed reads of conversion evidence in the shared IR.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use tracing::instrument;

use crate::error::CordialResult;
use crate::ir::{EdgeKind, IrMut, IrView, ItemKind, NodeKind, NodeWeight, QueryBuilder};
use crate::loader::SharedParsedFile;
use crate::objects::FileSpan;

use super::collection::collect_conversion_units;
use super::resolution::CrateTypeIndex;
use super::{ErrorConversionUnit, ErrorEdgeTargetKind};

const CONVERSION_UNIT_ATTR: &str = "error_conversion_unit";
const ERROR_DECLARATION_ATTR: &str = "error_apply_declaration";

/// Collect conversion evidence before apply and materialize it in the shared IR.
#[instrument(level = "debug", skip(ir, files), err(level = "warn"))]
pub(crate) fn enrich_conversion_units(
    ir: &mut dyn IrMut,
    files: &[SharedParsedFile],
) -> CordialResult<()> {
    let index = CrateTypeIndex::new(files);
    enrich_conversion_units_with_index(ir, files, &index)
}

#[instrument(level = "debug", skip(ir, files, index), err(level = "warn"))]
pub(in crate::etiquettes::internal_error_chain::apply) fn enrich_conversion_units_with_index(
    ir: &mut dyn IrMut,
    files: &[SharedParsedFile],
    index: &CrateTypeIndex,
) -> CordialResult<()> {
    let root = ir.root()?;
    let mut declarations = HashMap::<(PathBuf, usize, usize), _>::new();
    for (unit, line) in collect_conversion_units(files, index) {
        let node = ir.insert_node(
            NodeWeight::new(NodeKind::Field)
                .with_name(unit.field_name.clone())
                .with_span(FileSpan::new(&unit.declaration, line, 1)),
        )?;
        let mut dependencies = HashSet::new();
        for target in unit.sites().iter().filter_map(|site| {
            site.target()
                .as_ref()
                .or_else(|| site.edge().as_ref()?.target().as_ref())
        }) {
            let key = (target.declaration().clone(), target.line(), target.column());
            let declaration = if let Some(existing) = declarations.get(&key) {
                *existing
            } else {
                let kind = match target.kind() {
                    ErrorEdgeTargetKind::Struct => ItemKind::Struct,
                    ErrorEdgeTargetKind::Enum => ItemKind::Enum,
                };
                let declaration = ir.insert_node(
                    NodeWeight::new(NodeKind::Item(kind)).with_span(FileSpan::new(
                        target.declaration(),
                        target.line() as u32,
                        target.column() as u32 + 1,
                    )),
                )?;
                ir.set_attr(
                    declaration,
                    ERROR_DECLARATION_ATTR,
                    serde_json::to_value(target)?,
                )?;
                ir.insert_edge(root, declaration, EdgeKind::Contains)?;
                declarations.insert(key.clone(), declaration);
                declaration
            };
            if dependencies.insert(key) {
                ir.insert_edge(node, declaration, EdgeKind::Depends)?;
            }
        }
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
