//! Graph-backed intermediate representation shared by loaders, enrichers, and probes.
//!
//! The IR is organized as a [`WorkspaceIr`] containing one [`CrateIr`] per
//! analyzed crate. Each crate graph stores [`NodeWeight`] values connected by
//! [`EdgeWeight`] values, plus indexes for stable lookup by path and attribute.
//!
//! Extension authors normally consume the trait views: [`IrView`] for read-only
//! probes and assessors, [`IrMut`] through [`CrateViewMut`] for enrichers, and
//! [`Query`] for reusable graph searches. Concrete graph types remain public so
//! built-in loaders and tests can construct snapshots, but hook code should
//! prefer the view traits.

#[cfg(feature = "rustdoc")]
mod attrs;
mod crate_kind;
#[cfg(feature = "rustdoc")]
mod crate_load;
mod edge;
mod error_family;
mod function_source;
mod graph;
mod indexes;
mod node;
mod origin;
mod query;
#[cfg(feature = "rustdoc")]
mod rustdoc_query;
mod semantic;
mod source_attributes;
mod source_calls;
pub use source_attributes::{
    ATTR_SOURCE_ATTRIBUTE, ATTR_SOURCE_FILE_ATTRIBUTES, SourceAttribute, SourceAttributeArgument,
    SourceAttributeArguments, SourceAttributedItem, SourceCrateLints, SourceFileAttributes,
    SourceInstrumentSyntax, is_cfg_test, is_gated_instrument_attr, is_instrument_attr,
};
#[cfg(feature = "homecoming_std")]
mod trait_impls;
mod view;
mod workspace;
#[cfg(feature = "impl_coverage")]
mod wrapper_query;

#[cfg(feature = "rustdoc")]
pub use attrs::{
    ATTR_ALIAS_TARGET, ATTR_ELICIT_COMPLETE, ATTR_ELICIT_COMPLETE_FACTORY, ATTR_IS_GENERIC,
    ATTR_IS_PUBLIC, ATTR_IS_UNSTABLE, ATTR_ITEM_NAME, ATTR_PUBLIC_METHODS, ATTR_QUALIFIED_PATH,
    ATTR_RUSTDOC_KIND, ATTR_TRAIT_IMPLS, ATTR_TRAIT_PREREQS, ATTR_WRAPS_FOREIGN,
};

pub use crate_kind::{ATTR_BINARY_ENTRY_PATHS, ATTR_CRATE_KINDS, CrateKind};
mod input_snapshot;
#[cfg(feature = "rustdoc")]
pub use crate_load::{load_crate_ir_if_missing, resolve_crate_root, shadow_dep_rustdoc_path};
pub use edge::{EdgeKind, EdgeWeight};
pub use error_family::{
    ATTR_FUNCTION_ERROR_FAMILY, ErrorFamilyBasis, ErrorFamilyGap, ErrorFamilyGapReason,
    ErrorFamilyMember, FunctionErrorFamily,
};
pub(crate) use function_source::native_signature_range;
pub use function_source::{
    ATTR_FUNCTION_SOURCE_SYNTAX, DeclaredFunctionOutput, FunctionBoundary, FunctionCallSyntax,
    FunctionNestedScope, FunctionReturn, FunctionReturnFlow, FunctionSourceSyntax,
    FunctionSuccessExit, FunctionTail, FunctionUnwrapSyntax, NestedScopeKind, ReturnValue,
    SourceByteRange, SourceCallUse, SourceEntryContract, SourceResultContract,
};
pub use graph::{CrateIr, CrateIrSnapshot};
pub use indexes::{AttrKey, AttrValue, IrIndexes, QualifiedPath};
pub use input_snapshot::{ATTR_CARGO_MANIFEST_SNAPSHOT, FileSnapshot};
pub use node::{ItemKind, NodeId, NodeKind, NodeWeight};
pub use origin::{ATTR_IR_ORIGIN, ATTR_SYN_DOC_PEER, ORIGIN_RUSTDOC, ORIGIN_SOURCE};
pub use query::{BasicQuery, PanicSitesQuery, Query, QueryBuilder};
#[cfg(feature = "rustdoc")]
pub use rustdoc_query::{
    count_type_nodes, mirror_target, rustdoc_item_nodes, type_elicit_complete, type_public_methods,
    type_trait_impls, type_trait_prereqs, type_wraps_foreign,
};
pub use semantic::{
    ATTR_SEMANTIC_FUNCTION, ATTR_SEMANTIC_SNAPSHOT, SemanticCall, SemanticCallTarget,
    SemanticCapability, SemanticFormattingRole, SemanticFunction, SemanticItemId,
    SemanticResultRecording, SemanticReturnContract, SemanticSnapshot, SemanticType,
};
pub use source_calls::{
    ATTR_FUNCTION_CALL_FLOW, ATTR_SOURCE_MODULE_SCOPE, CallResolutionFailure, FunctionCallFlow,
    ResolvedFunctionCall, SourceCallResolution, SourceModuleScope, SourcePathUse,
    SourceUseInventory,
};
#[cfg(feature = "homecoming_std")]
pub use trait_impls::collect_trait_impl_type_paths_from_ir;
pub use view::{IrMut, IrView, NodeRef, NodeView};
pub use workspace::{CrateView, CrateViewMut, WorkspaceIr};
#[cfg(feature = "impl_coverage")]
pub use wrapper_query::{
    build_wrapper_coverage_from_hub_ir, collect_trenchcoat_pairs_from_ir, wrapper_maps_equivalent,
};
