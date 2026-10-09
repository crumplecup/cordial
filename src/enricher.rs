use tracing::instrument;
#[cfg(feature = "_attribute_scan")]
mod attribute;
#[cfg(feature = "_attribute_scan")]
mod attribute_paths;
#[cfg(feature = "error_sites")]
mod error;
#[cfg(feature = "error_sites")]
mod error_flow;
#[cfg(feature = "impl_coverage")]
mod feature_probe;
#[cfg(feature = "_quality_support")]
mod ir_paths;
mod path_index;
#[cfg(feature = "impl_coverage")]
mod proof_harness;
#[cfg(feature = "rustdoc")]
mod rustdoc_structure;
#[cfg(feature = "shadow")]
mod shadow;
mod syn_doc_link;
#[cfg(feature = "impl_coverage")]
mod trait_impl;
#[cfg(feature = "trenchcoat")]
mod trenchcoat;
#[cfg(feature = "impl_coverage")]
mod wrapper_coverage;

#[cfg(feature = "_attribute_scan")]
pub use attribute::{AttributeEnricher, is_cfg_test, is_gated_instrument_attr, is_instrument_attr};
#[cfg(feature = "_quality_support")]
#[cfg(feature = "_attribute_scan")]
pub use attribute_paths::{member_crate_root, resolve_parent};
#[cfg(feature = "_attribute_scan")]
#[cfg(feature = "error_sites")]
pub use error::{
    ERROR_IR_ENRICHERS, ErrorIrScanEnricher, ErrorIrScanReport, error_ir_enricher_ids,
    scan_crate_error_ir,
};
#[cfg(feature = "error_sites")]
pub use error_flow::ErrorFlowEnricher;
#[cfg(feature = "impl_coverage")]
pub use feature_probe::FeatureProbeEnricher;
#[cfg(feature = "_quality_support")]
pub use ir_paths::resolve_source_path;
pub use path_index::PathIndexEnricher;
#[cfg(feature = "impl_coverage")]
pub use proof_harness::ProofHarnessEnricher;
#[cfg(feature = "rustdoc")]
pub use rustdoc_structure::RustdocStructureEnricher;
#[cfg(feature = "shadow")]
pub use shadow::{
    ShadowLinkEnricher, ShadowMapEntry, discover_same_crate_shadow_pairs, load_shadow_map,
    resolve_shadow_entries,
};
#[cfg(feature = "rustdoc")]
pub use syn_doc_link::inventory_link_key;
pub use syn_doc_link::{SynDocLinkEnricher, syn_doc_peer};
#[cfg(feature = "impl_coverage")]
pub use trait_impl::TraitImplEnricher;
#[cfg(feature = "trenchcoat")]
pub use trenchcoat::TrenchcoatEnricher;
#[cfg(feature = "impl_coverage")]
pub use wrapper_coverage::WrapperCoverageEnricher;

use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{BasicQuery, EdgeKind, IrView, NodeKind};

/// Adds scope edges from items to their enclosing module.
#[derive(Debug, Default, Clone, Copy)]
pub struct ScopeEnricher;

impl ScopeEnricher {
    /// Stable identifier for `ScopeEnricher`.
    pub const ID: &'static str = "scope";
}

impl IrEnricher for ScopeEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        0
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir();

        let all_nodes = BasicQuery::all_nodes();
        let items: Vec<_> = ir
            .nodes_matching(&all_nodes)
            .into_iter()
            .filter(|node| matches!(node.kind(), NodeKind::Item(_)))
            .map(|node| node.id())
            .collect();

        for item in items {
            if let Some(module) = find_enclosing_module(ir, item) {
                ir.insert_edge(item, module, EdgeKind::Scope)?;
            }
        }
        Ok(())
    }
}

#[instrument(level = "debug", skip(ir, item))]
fn find_enclosing_module(ir: &dyn IrView, item: crate::ir::NodeId) -> Option<crate::ir::NodeId> {
    let mut current = item;
    loop {
        let parents = ir.parents(current, EdgeKind::Contains);
        if parents.is_empty() {
            return None;
        }
        for parent in parents {
            if let Some(node) = ir.node(parent)
                && matches!(node.kind(), NodeKind::Module)
            {
                return Some(parent);
            }
            current = parent;
        }
    }
}
