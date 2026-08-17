mod elicit_complete;
mod impls;
mod inventory;
mod method_maps;
mod prereqs;
mod public_extract;
mod stability;
mod trenchcoat;
#[cfg(feature = "impl_coverage")]
mod workspace_wrapper;
mod wrapper_coverage;

pub use elicit_complete::{
    ElicitCompleteSet, collect_elicit_complete_from_inventory, collect_elicit_complete_paths,
};
#[cfg(feature = "elicitation")]
pub use fixture::{
    demo_impl_coverage_crate, demo_shadow_crate, demo_trenchcoat_crate, write_rustdoc_crate_json,
};
pub use impls::{TraitImplRecord, collect_trait_impls};
pub use inventory::{
    InventoryItemKind, RustdocInventory, RustdocItem, ir_item_kind, parse_rustdoc_json,
};
pub use method_maps::{
    collect_trait_impl_map, collect_trait_impl_map_from_inventory, collect_type_methods,
    collect_type_methods_from_inventory, methods_for_type_path,
};
pub use prereqs::{
    ELICIT_COMPLETE_SUPERTRAITS, ELICIT_COMPLETE_TRAIT, TraitPrereqs,
    collect_trait_prereqs_for_inventory, prereqs_from_trait_shorts,
};
pub use public_extract::{ExtractedItem, extract_public_items};
pub use stability::{
    StabilityLevel, item_attrs_are_unstable, parse_stability_attr_text,
    rustdoc_json_has_stability_markers, stability_from_attrs,
};
pub use trenchcoat::{TrenchcoatPair, collect_trenchcoat_pairs};
#[cfg(feature = "impl_coverage")]
pub use workspace_wrapper::ensure_workspace_wrapper_coverage;
pub use wrapper_coverage::{
    WrapperCoverage, WrapperCoverageMap, build_wrapper_coverage_map, coverage_provider_label,
    covered_indirectly, effective_missing_our_traits, indirect_elicit_complete, join_wrapper_paths,
    lookup_wrapper_coverage,
};

#[cfg(feature = "elicitation")]
mod fixture;
