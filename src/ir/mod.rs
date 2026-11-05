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
#[cfg(feature = "internal_error_chain")]
mod error_architecture_source;
#[cfg(feature = "error_chain")]
mod error_chains_source;
#[cfg(feature = "internal_error_chain")]
mod error_compliance_source;
mod error_family;
#[cfg(feature = "internal_error_chain")]
mod error_signatures_source;
#[cfg(feature = "internal_error_chain")]
pub(crate) use {
    error_chains_source::types::type_label as native_error_type_label,
    error_sites_source::native::{excluded_module, path_label as native_error_path_label},
    error_types_source::derives_error,
};
#[cfg(feature = "internal_error_chain")]
mod error_types_source;
#[cfg(feature = "error_chain")]
pub use error_chains_source::{ATTR_SOURCE_ERROR_CHAINS, ErrorChainsFile, SourceErrorChains};
#[cfg(feature = "internal_error_chain")]
pub use {
    error_architecture_source::{ATTR_SOURCE_ERROR_ARCHITECTURE, SourceErrorArchitecture},
    error_compliance_source::{
        ATTR_SOURCE_ERROR_COMPLIANCE, ErrorComplianceFile, SourceErrorCompliance,
    },
    error_signatures_source::{
        ATTR_SOURCE_ERROR_SIGNATURES, ErrorSignatureAssessment, SourceErrorSignatures,
    },
    error_types_source::{
        ATTR_SOURCE_ERROR_TYPES, ErrorTypeSourceNode, ErrorTypesFile, SourceErrorTypes,
    },
};
#[cfg(feature = "error_sites")]
mod error_sites_source;
#[cfg(feature = "error_sites")]
pub use error_sites_source::{ATTR_SOURCE_ERROR_SITES, ErrorSitesFile, SourceErrorSites};
mod function_source;
mod graph;
mod indexes;
mod node;
mod origin;
#[cfg(feature = "derives")]
mod path_inclusion;
#[cfg(feature = "derives")]
pub use path_inclusion::{PathInclusionFacts, PathInclusionMember, PathInclusionSource};
#[cfg(feature = "derives")]
pub(crate) use path_inclusion::{canonical_or, normalize};
#[cfg(feature = "tracing")]
mod proof_calls;
#[cfg(feature = "tracing")]
pub use proof_calls::SourceProofFunction;
mod query;
#[cfg(feature = "rustdoc")]
mod rustdoc_query;
mod semantic;
mod semantic_workspace;
mod source_attributes;
mod source_calls;
#[cfg(any(
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "visibility",
    feature = "cli_layout"
))]
mod source_policy;
#[cfg(feature = "allows")]
pub use source_policy::scan_allows_source;
#[cfg(feature = "cfg_hygiene")]
pub use source_policy::scan_cfg_hygiene_source;
#[cfg(feature = "cfg_scatter")]
pub use source_policy::scan_cfg_scatter_source;
#[cfg(feature = "glob_imports")]
pub use source_policy::scan_glob_imports_source;
#[cfg(feature = "inline_tests")]
pub use source_policy::scan_inline_tests_source;
#[cfg(feature = "modularity")]
pub use source_policy::scan_modularity_source;
#[cfg(feature = "pageantry")]
pub use source_policy::scan_pageantry_source;
#[cfg(any(
    feature = "allows",
    feature = "cfg_hygiene",
    feature = "cfg_scatter",
    feature = "glob_imports",
    feature = "inline_tests",
    feature = "modularity",
    feature = "pageantry",
    feature = "derives",
    feature = "visibility",
    feature = "cli_layout"
))]
pub use source_policy::{ATTR_SOURCE_POLICY_INVENTORY, SourcePolicyInventory};
#[cfg(feature = "derives")]
pub use source_policy::{
    DeriveFieldFacts, DeriveFieldRead, DeriveFileFacts, DeriveImplFacts, DeriveItemFacts,
    DeriveMethodFacts, DeriveMethodFlags, DeriveModuleFacts, DeriveSetterShape, DeriveStructFacts,
    scan_derives_source,
};
#[cfg(feature = "visibility")]
pub use source_policy::{VisibilityFacts, VisibilityKind, VisibilityModule};
#[cfg(feature = "tracing")]
mod observability_source;
#[cfg(feature = "tracing")]
mod tracing_source;
#[cfg(feature = "tracing")]
pub use observability_source::{ATTR_SOURCE_OBSERVABILITY, SourceObservability};
#[cfg(feature = "tracing")]
pub(crate) use observability_source::{
    ObservabilityBody, ObservabilityFile, ObservabilityFileKind,
};
pub use source_attributes::{
    ATTR_SOURCE_ATTRIBUTE, ATTR_SOURCE_FILE_ATTRIBUTES, SourceAttribute, SourceAttributeArgument,
    SourceAttributeArguments, SourceAttributedItem, SourceCrateLints, SourceFileAttributes,
    SourceInstrumentSyntax,
};
#[cfg(feature = "_attribute_scan")]
pub use source_attributes::{is_cfg_test, is_gated_instrument_attr, is_instrument_attr};
#[cfg(feature = "tracing")]
pub use tracing_source::{
    ATTR_SOURCE_TRACING_INVENTORY, SourceTracingFunction, SourceTracingInventory,
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
pub use node::{NodeRef, NodeView};
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
pub use semantic_workspace::{
    SemanticConfigurationInput, SemanticContextGap, SemanticCrateContext, SemanticFeatures,
    SemanticWorkspaceContext,
};
pub use source_calls::{
    ATTR_FUNCTION_CALL_FLOW, ATTR_SOURCE_MODULE_SCOPE, CallResolutionFailure, FunctionCallFlow,
    ResolvedFunctionCall, SourceCallResolution, SourceModuleScope, SourcePathUse,
    SourceUseInventory,
};
#[cfg(feature = "homecoming_std")]
pub use trait_impls::collect_trait_impl_type_paths_from_ir;
pub use view::{IrMut, IrView};
pub use workspace::{CrateView, CrateViewMut, WorkspaceIr};
#[cfg(feature = "impl_coverage")]
pub use wrapper_query::{
    build_wrapper_coverage_from_hub_ir, collect_trenchcoat_pairs_from_ir, wrapper_maps_equivalent,
};

#[cfg(feature = "cli_layout")]
pub use source_policy::CliLayoutFacts;
#[cfg(feature = "cli_layout")]
pub(crate) use source_policy::{EntryItem, EntryKind, LayoutCatalog, TypeRec, VariantShape};
