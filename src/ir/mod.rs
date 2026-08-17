mod attrs;
#[cfg(feature = "rustdoc")]
mod crate_load;
mod edge;
mod graph;
mod indexes;
mod node;
mod origin;
mod query;
#[cfg(feature = "rustdoc")]
mod rustdoc_query;
mod trait_impls;
mod view;
mod workspace;
#[cfg(feature = "impl_coverage")]
mod wrapper_query;

pub use attrs::*;

#[cfg(feature = "rustdoc")]
pub use crate_load::{load_crate_ir_if_missing, resolve_crate_root, shadow_dep_rustdoc_path};
pub use edge::{EdgeKind, EdgeWeight};
pub use graph::{CrateIr, CrateIrSnapshot};
pub use indexes::{AttrKey, AttrValue, IrIndexes, QualifiedPath};
pub use node::{ItemKind, NodeId, NodeKind, NodeWeight};
pub use origin::{ATTR_IR_ORIGIN, ATTR_SYN_DOC_PEER, ORIGIN_RUSTDOC, ORIGIN_SOURCE};
pub use query::{BasicQuery, PanicSitesQuery, Query, QueryBuilder};
#[cfg(feature = "rustdoc")]
pub use rustdoc_query::{
    count_type_nodes, mirror_target, rustdoc_item_nodes, type_elicit_complete, type_public_methods,
    type_trait_impls, type_trait_prereqs, type_wraps_foreign,
};
pub use trait_impls::collect_trait_impl_type_paths_from_ir;
pub use view::{IrMut, IrView, NodeRef, NodeView};
pub use workspace::{CrateView, CrateViewMut, WorkspaceIr};
#[cfg(feature = "impl_coverage")]
pub use wrapper_query::{
    build_wrapper_coverage_from_hub_ir, collect_trenchcoat_pairs_from_ir, wrapper_maps_equivalent,
};
