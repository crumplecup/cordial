use crate::error::CordialResult;
use crate::hooks::IrEnricher;
use crate::ir::{
    ATTR_QUALIFIED_PATH, ATTR_TRAIT_IMPLS, BasicQuery, EdgeKind, IrMut, NodeKind, NodeWeight,
};
use crate::loader::LoadView;
use crate::session::SessionView;

use tracing::instrument;
/// Adds [`EdgeKind::Implements`] edges from materialized `trait_impls` attrs.
#[derive(Debug, Default, Clone, Copy)]
pub struct TraitImplEnricher;

impl TraitImplEnricher {
    pub const ID: &'static str = "trait-impl";
}

impl IrEnricher for TraitImplEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        3
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

        let types: Vec<(crate::ir::NodeId, Vec<String>)> = ir
            .nodes_matching(&ALL_NODES)
            .into_iter()
            .filter_map(|node| {
                if !matches!(node.kind(), NodeKind::Item(_)) {
                    return None;
                }
                if node.attr(ATTR_QUALIFIED_PATH).is_none() {
                    return None;
                }
                let trait_shorts: Vec<String> = node
                    .attr(ATTR_TRAIT_IMPLS)
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                if trait_shorts.is_empty() {
                    return None;
                }
                Some((node.id, trait_shorts))
            })
            .collect();

        for (type_node, trait_shorts) in types {
            for trait_short in trait_shorts {
                let trait_node = ensure_trait_node(ir, &trait_short)?;
                ir.insert_edge(type_node, trait_node, EdgeKind::Implements)?;
            }
        }
        Ok(())
    }
}

#[instrument(level = "debug", skip(ir), err(level = "warn"))]
fn ensure_trait_node(ir: &mut dyn IrMut, trait_short: &str) -> CordialResult<crate::ir::NodeId> {
    if let Some(existing) = ir
        .nodes_matching(&BasicQuery::all_nodes())
        .into_iter()
        .find(|node| {
            node.attr("trait_short")
                .and_then(|value| value.as_str())
                .is_some_and(|short| short == trait_short)
        })
    {
        return Ok(existing.id);
    }
    let node = ir.insert_node(
        NodeWeight::new(NodeKind::Item(crate::ir::ItemKind::Trait))
            .with_name(trait_short.to_string()),
    )?;
    ir.set_attr(
        node,
        ATTR_QUALIFIED_PATH,
        serde_json::Value::String(trait_short.to_string()),
    )?;
    ir.set_attr(
        node,
        "trait_short",
        serde_json::Value::String(trait_short.to_string()),
    )?;
    Ok(node)
}
