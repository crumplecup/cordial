use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{
    ATTR_QUALIFIED_PATH, ATTR_WRAPS_FOREIGN, BasicQuery, EdgeKind, IrMut, NodeKind, NodeWeight,
};
use crate::loader::LoadView;
use crate::session::SessionView;

use tracing::instrument;
/// Adds [`EdgeKind::Wraps`] edges from materialized `wraps_foreign` attrs.
#[derive(Debug, Default, Clone, Copy)]
pub struct TrenchcoatEnricher;

impl TrenchcoatEnricher {
    pub const ID: &'static str = "trenchcoat";
}

impl IrEnricher for TrenchcoatEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        4
    }

    #[instrument(level = "trace", skip(self))]
    fn required_loader(&self) -> &str {
        crate::RustdocLoader::ID
    }

    #[instrument(level = "trace", skip(self, ir, _load, _session))]
    fn enrich(
        &self,
        ir: &mut dyn IrMut,
        _load: &dyn LoadView,
        _session: &dyn SessionView,
    ) -> CordialResult<()> {
        static ALL_NODES: BasicQuery = BasicQuery {
            node_kinds: Vec::new(),
            edge_kinds: Vec::new(),
            attr_key: None,
            attr_value: None,
        };

        let wrappers: Vec<(crate::ir::NodeId, String)> = ir
            .nodes_matching(&ALL_NODES)
            .into_iter()
            .filter_map(|node| {
                if !matches!(node.kind(), NodeKind::Item(_)) {
                    return None;
                }
                let foreign = node
                    .attr(ATTR_WRAPS_FOREIGN)
                    .and_then(|value| value.as_str())
                    .map(str::to_string)?;
                Some((node.id, foreign))
            })
            .collect();

        for (wrapper, foreign_path) in wrappers {
            let foreign = ensure_type_node(ir, &foreign_path)?;
            if !has_wraps_edge(ir, wrapper, foreign) {
                ir.insert_edge(wrapper, foreign, EdgeKind::Wraps)?;
            }
        }
        Ok(())
    }
}

#[instrument(level = "trace", skip(ir, wrapper, foreign), ret)]
fn has_wraps_edge(
    ir: &dyn crate::ir::IrView,
    wrapper: crate::ir::NodeId,
    foreign: crate::ir::NodeId,
) -> bool {
    ir.children(wrapper, EdgeKind::Wraps)
        .into_iter()
        .any(|target| target == foreign)
}

#[instrument(level = "debug", skip(ir, path), err(level = "warn"))]
fn ensure_type_node(ir: &mut dyn IrMut, path: &str) -> CordialResult<crate::ir::NodeId> {
    if let Some(existing) = ir.node_by_path(path) {
        return Ok(existing);
    }
    let name = path.rsplit("::").next().unwrap_or("Type").to_string();
    let node = ir.insert_node(
        NodeWeight::new(NodeKind::Item(crate::ir::ItemKind::Struct)).with_name(name),
    )?;
    ir.set_attr(
        node,
        ATTR_QUALIFIED_PATH,
        serde_json::Value::String(path.to_string()),
    )?;
    Ok(node)
}
