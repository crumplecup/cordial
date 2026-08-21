//! Unified syn visitor for error-handling IR scans (sites, chain, compliance).
//!
//! `error_sites` logic lives here unconditionally. `error_chain` and
//! `internal_error_chain` logic live in their own modules
//! (`chain_layer`, `compliance_layer`), each gated as a whole unit by a
//! single `#[cfg(feature = ...)]` on the `mod` declaration in
//! `error_ir/mod.rs`. This file only needs a handful of cfg attributes at
//! the boundary where it holds or calls into those layers — never inside a
//! shared helper function.

use std::path::Path;

use syn::visit::Visit;

#[cfg(feature = "internal_error_chain")]
use std::collections::BTreeSet;

#[cfg(feature = "internal_error_chain")]
use super::compliance_layer::ComplianceLayer;
use crate::etiquettes::error_sites::ErrorSiteRecord;
#[cfg(feature = "internal_error_chain")]
use crate::etiquettes::internal_error_chain::{
    InternalErrorComplianceFinding, RawTypeNode, scan_error_rust_syntax_raw,
};
use crate::loader::module_path_from_src_file;
use tracing::instrument;
#[cfg(feature = "error_chain")]
use {super::chain_layer::ChainLayer, crate::etiquettes::error_chain::ErrorChainRecord};

mod expr;
#[cfg(any(feature = "error_chain", feature = "internal_error_chain"))]
mod site;
mod walk;

pub(super) use expr::{pat_is_err, raw_expr_snippet, truncate_snippet};
#[cfg(any(feature = "error_chain", feature = "internal_error_chain"))]
pub(super) use site::SiteCtx;
use walk::ErrorIrUnifiedVisitor;

/// Which error IR layers to collect during a unified file scan. Plain
/// `bool`s carry no feature-gated type, so they stay unconditional even
/// though only some are meaningful when a layer's feature is disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorIrScanLayers {
    pub sites: bool,
    pub chain: bool,
    pub compliance: bool,
    pub type_graph: bool,
}

impl ErrorIrScanLayers {
    pub const SITES_ONLY: Self = Self {
        sites: true,
        chain: false,
        compliance: false,
        type_graph: false,
    };

    pub const CHAIN_ONLY: Self = Self {
        sites: false,
        chain: true,
        compliance: false,
        type_graph: false,
    };

    pub const COMPLIANCE_ONLY: Self = Self {
        sites: false,
        chain: false,
        compliance: true,
        type_graph: false,
    };

    #[instrument(level = "debug")]
    pub fn for_unified_file(under_src: bool) -> Self {
        Self {
            sites: true,
            chain: under_src,
            compliance: under_src,
            type_graph: under_src,
        }
    }
}

/// Combined scan output for one source file.
#[derive(Debug, Default)]
pub struct ErrorIrFileScan {
    pub sites: Vec<ErrorSiteRecord>,
    #[cfg(feature = "error_chain")]
    pub chain: Vec<ErrorChainRecord>,
    #[cfg(feature = "internal_error_chain")]
    pub compliance: Vec<InternalErrorComplianceFinding>,
    #[cfg(feature = "internal_error_chain")]
    pub type_graph_raw: Vec<RawTypeNode>,
    #[cfg(feature = "internal_error_chain")]
    pub error_impls: BTreeSet<String>,
}

/// Scan a pre-parsed file for error-handling IR facts (one AST walk for sites/chain/compliance).
#[instrument(level = "debug", skip(syntax, file, layers))]
pub fn scan_rust_file_syntax(
    syntax: &syn::File,
    file: &Path,
    tree_root: &Path,
    src_root: &Path,
    crate_root: &Path,
    crate_name: &str,
    layers: ErrorIrScanLayers,
) -> ErrorIrFileScan {
    let module_prefix = module_path_from_src_file(tree_root, file);
    let mut visitor = ErrorIrUnifiedVisitor {
        layers,
        crate_name: crate_name.to_string(),
        file: file.to_path_buf(),
        crate_root: crate_root.to_path_buf(),
        module_prefix,
        impl_type: None,
        fn_stack: Vec::new(),
        sites: Vec::new(),
        #[cfg(feature = "error_chain")]
        chain_layer: ChainLayer::new(),
        #[cfg(feature = "internal_error_chain")]
        compliance_layer: ComplianceLayer::new(),
    };
    visitor.visit_file(syntax);

    let mut scan = ErrorIrFileScan {
        sites: visitor.sites,
        #[cfg(feature = "error_chain")]
        chain: visitor.chain_layer.into_records(),
        #[cfg(feature = "internal_error_chain")]
        compliance: visitor.compliance_layer.into_findings(),
        #[cfg(feature = "internal_error_chain")]
        type_graph_raw: Vec::new(),
        #[cfg(feature = "internal_error_chain")]
        error_impls: BTreeSet::new(),
    };

    #[cfg(feature = "internal_error_chain")]
    if layers.type_graph {
        let graph = scan_error_rust_syntax_raw(syntax, file, src_root);
        scan.type_graph_raw = graph.nodes;
        scan.error_impls = graph.error_impls;
    }

    scan
}
