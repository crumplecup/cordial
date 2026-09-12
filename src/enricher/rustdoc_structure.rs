use std::collections::{BTreeSet, HashMap};

use crate::RustdocLoadView;
use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{
    ATTR_ALIAS_TARGET, ATTR_ELICIT_COMPLETE, ATTR_ELICIT_COMPLETE_FACTORY, ATTR_IS_GENERIC,
    ATTR_IS_PUBLIC, ATTR_IS_UNSTABLE, ATTR_ITEM_NAME, ATTR_PUBLIC_METHODS, ATTR_QUALIFIED_PATH,
    ATTR_TRAIT_IMPLS, ATTR_TRAIT_PREREQS, ATTR_WRAPS_FOREIGN, BasicQuery, NodeKind,
};
use crate::rustdoc::{
    collect_elicit_complete_from_inventory, collect_trait_impls,
    collect_trait_prereqs_for_inventory, collect_trenchcoat_pairs,
    collect_type_methods_from_inventory, extract_public_items, methods_for_type_path,
};

use tracing::instrument;
/// Materializes rustdoc JSON facts onto type/trait item nodes as attrs.
#[derive(Debug, Default, Clone, Copy)]
pub struct RustdocStructureEnricher;

impl RustdocStructureEnricher {
    /// Stable identifier for `RustdocStructureEnricher`.
    pub const ID: &'static str = "rustdoc-structure";
}

impl IrEnricher for RustdocStructureEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        1
    }

    #[instrument(level = "trace", skip(self))]
    fn required_loader(&self) -> &str {
        crate::RustdocLoader::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let (ir, load, _) = view.into_parts();

        let Some(rustdoc) = load.as_any().downcast_ref::<RustdocLoadView>() else {
            return Ok(());
        };

        let inventory = rustdoc.inventory();
        let methods = collect_type_methods_from_inventory(inventory);
        let prereqs = collect_trait_prereqs_for_inventory(inventory);
        let elicit_complete = collect_elicit_complete_from_inventory(inventory);
        let trenchcoat_pairs = collect_trenchcoat_pairs(inventory);

        let mut trait_impls_by_type: HashMap<String, BTreeSet<String>> = HashMap::new();
        for record in collect_trait_impls(inventory) {
            let (type_path, trait_short) = record.into_type_and_trait_short();
            trait_impls_by_type
                .entry(type_path)
                .or_default()
                .insert(trait_short);
        }

        let extracted: HashMap<String, _> =
            extract_public_items(inventory.krate(), inventory.crate_name(), false)
                .into_iter()
                .map(|item| (item.path_str(), item))
                .collect();

        let items_by_path: HashMap<&str, _> = inventory
            .items()
            .iter()
            .map(|item| (item.path().as_str(), item))
            .collect();

        let all_nodes = BasicQuery::all_nodes();
        let nodes: Vec<(crate::ir::NodeId, String)> = ir
            .nodes_matching(&all_nodes)
            .into_iter()
            .filter_map(|node| {
                if !matches!(node.kind(), NodeKind::Item(_)) {
                    return None;
                }
                let path = node
                    .attr(ATTR_QUALIFIED_PATH)
                    .and_then(|value| value.as_str())
                    .map(str::to_string)?;
                Some((node.id(), path))
            })
            .collect();

        for (node_id, path) in nodes {
            if let Some(item) = items_by_path.get(path.as_str()) {
                ir.set_attr(
                    node_id,
                    ATTR_ITEM_NAME,
                    serde_json::Value::String(item.name().clone()),
                )?;
                ir.set_attr(
                    node_id,
                    ATTR_IS_PUBLIC,
                    serde_json::Value::Bool(item.is_public()),
                )?;
            }

            if let Some(meta) = extracted.get(&path) {
                ir.set_attr(
                    node_id,
                    ATTR_IS_GENERIC,
                    serde_json::Value::Bool(meta.is_generic()),
                )?;
                ir.set_attr(
                    node_id,
                    ATTR_IS_UNSTABLE,
                    serde_json::Value::Bool(meta.is_unstable()),
                )?;
                if let Some(target) = meta.alias_target() {
                    ir.set_attr(
                        node_id,
                        ATTR_ALIAS_TARGET,
                        serde_json::Value::String(target.clone()),
                    )?;
                }
            }

            let is_type = items_by_path
                .get(path.as_str())
                .is_some_and(|item| item.kind().is_type());

            if is_type {
                let public_methods: Vec<String> =
                    methods_for_type_path(&path, &methods).into_iter().collect();
                if !public_methods.is_empty() {
                    ir.set_attr(
                        node_id,
                        ATTR_PUBLIC_METHODS,
                        serde_json::json!(public_methods),
                    )?;
                }

                if let Some(impls) = trait_impls_by_type.get(&path) {
                    let trait_impls: Vec<String> = impls.iter().cloned().collect();
                    ir.set_attr(node_id, ATTR_TRAIT_IMPLS, serde_json::json!(trait_impls))?;
                }

                if let Some(type_prereqs) = prereqs.get(&path) {
                    ir.set_attr(
                        node_id,
                        ATTR_TRAIT_PREREQS,
                        serde_json::to_value(type_prereqs)?,
                    )?;
                }

                ir.set_attr(
                    node_id,
                    ATTR_ELICIT_COMPLETE,
                    serde_json::Value::Bool(elicit_complete.contains_path(&path)),
                )?;
                ir.set_attr(
                    node_id,
                    ATTR_ELICIT_COMPLETE_FACTORY,
                    serde_json::Value::Bool(elicit_complete.contains_factory_path(&path)),
                )?;
            }
        }

        for pair in trenchcoat_pairs {
            let wrapper_path = pair.wrapper_path().clone();
            let Some(wrapper) = ir.node_by_path(&wrapper_path) else {
                continue;
            };
            ir.set_attr(
                wrapper,
                ATTR_WRAPS_FOREIGN,
                serde_json::Value::String(pair.foreign_path().clone()),
            )?;
        }

        Ok(())
    }
}
