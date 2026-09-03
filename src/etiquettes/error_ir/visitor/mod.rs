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

use crate::error::CordialResult;
#[cfg(feature = "error_chain")]
use crate::etiquettes::error_chain::ErrorChainRecord;
use crate::etiquettes::error_sites::ErrorSiteRecord;
#[cfg(feature = "internal_error_chain")]
use crate::etiquettes::internal_error_chain::{
    InternalErrorComplianceFinding, RawTypeNode, scan_error_rust_syntax_raw,
};
use crate::loader::module_path_from_src_file;
use tracing::instrument;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct ErrorIrScanLayers {
    #[getter(copy)]
    sites: bool,
    #[getter(copy)]
    chain: bool,
    #[getter(copy)]
    compliance: bool,
    #[getter(copy)]
    type_graph: bool,
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
#[derive(Debug, Default, derive_getters::Getters)]
pub struct ErrorIrFileScan {
    sites: Vec<ErrorSiteRecord>,
    #[cfg(feature = "error_chain")]
    chain: Vec<ErrorChainRecord>,
    #[cfg(feature = "internal_error_chain")]
    compliance: Vec<InternalErrorComplianceFinding>,
    #[cfg(feature = "internal_error_chain")]
    type_graph_raw: Vec<RawTypeNode>,
    #[cfg(feature = "internal_error_chain")]
    error_impls: BTreeSet<String>,
}

impl ErrorIrFileScan {
    #[cfg(feature = "internal_error_chain")]
    pub(super) fn with_type_graph(
        mut self,
        type_graph_raw: Vec<RawTypeNode>,
        error_impls: BTreeSet<String>,
    ) -> Self {
        self.type_graph_raw = type_graph_raw;
        self.error_impls = error_impls;
        self
    }

    pub(super) fn from_parts(
        sites: Vec<ErrorSiteRecord>,
        #[cfg(feature = "error_chain")] chain: Vec<ErrorChainRecord>,
        #[cfg(feature = "internal_error_chain")] compliance: Vec<InternalErrorComplianceFinding>,
        #[cfg(feature = "internal_error_chain")] type_graph_raw: Vec<RawTypeNode>,
        #[cfg(feature = "internal_error_chain")] error_impls: BTreeSet<String>,
    ) -> Self {
        Self {
            sites,
            #[cfg(feature = "error_chain")]
            chain,
            #[cfg(feature = "internal_error_chain")]
            compliance,
            #[cfg(feature = "internal_error_chain")]
            type_graph_raw,
            #[cfg(feature = "internal_error_chain")]
            error_impls,
        }
    }
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
) -> CordialResult<ErrorIrFileScan> {
    let module_prefix = module_path_from_src_file(tree_root, file);
    let mut visitor = ErrorIrUnifiedVisitor::new(
        layers,
        crate_name.to_string(),
        file.to_path_buf(),
        crate_root.to_path_buf(),
        module_prefix,
    );
    visitor.visit_file(syntax);

    let scan = visitor.into_file_scan()?;

    #[cfg(feature = "internal_error_chain")]
    if layers.type_graph() {
        let graph = scan_error_rust_syntax_raw(syntax, file, src_root)?;
        return Ok(scan.with_type_graph(graph.nodes().clone(), graph.error_impls().clone()));
    }

    Ok(scan)
}
