//! Compatibility syn visitor for chain and compliance IR scans.
//!
//! Error sites use the native source database. `error_chain` and
//! `internal_error_chain` logic still live in their own modules
//! (`chain_layer`, `compliance_layer`), each gated as a whole unit by a
//! single `#[cfg(feature = ...)]` on the `mod` declaration in
//! `error_ir/mod.rs`. This file only needs a handful of cfg attributes at
//! the boundary where it holds or calls into those layers — never inside a
//! shared helper function.

use std::path::Path;

use syn::visit::Visit;

use crate::error::CordialResult;
use crate::loader::module_path_from_src_file;
use tracing::instrument;

pub(super) mod expr;
#[cfg(any(feature = "error_chain", feature = "internal_error_chain"))]
mod site;
mod walk;

#[cfg(any(feature = "error_chain", feature = "internal_error_chain"))]
pub(super) use site::SiteCtx;
use walk::ErrorIrUnifiedVisitor;

/// Which error IR layers to collect during a unified file scan. Plain
/// `bool`s carry no feature-gated type, so they stay unconditional even
/// though only some are meaningful when a layer's feature is disabled.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, derive_builder::Builder, derive_getters::Getters,
)]
#[builder(default, build_fn(error = "crate::error::CordialError"))]
pub struct ErrorIrScanLayers {
    #[getter(copy)]
    chain: bool,
    #[getter(copy)]
    compliance: bool,
    /// Whether the crate being scanned builds as a `proc-macro = true`
    /// library. Only meaningful to the `compliance` layer, which uses it
    /// to recognize `syn::Error` as that kind of crate's real diagnostic
    /// terminus rather than a dropped error chain.
    #[getter(copy)]
    is_proc_macro_crate: bool,
}

/// Combined scan output for one source file.
#[derive(Debug, Default, derive_getters::Getters)]
pub struct ErrorIrFileScan {
    #[cfg(feature = "error_chain")]
    chain: Vec<crate::etiquettes::error_chain::ErrorChainRecord>,
    #[cfg(feature = "internal_error_chain")]
    compliance: Vec<crate::etiquettes::internal_error_chain::InternalErrorComplianceFinding>,
}

impl ErrorIrFileScan {
    #[instrument(level = "debug", skip(chain, compliance))]
    pub(super) fn from_parts(
        #[cfg(feature = "error_chain")] chain: Vec<
            crate::etiquettes::error_chain::ErrorChainRecord,
        >,
        #[cfg(feature = "internal_error_chain")] compliance: Vec<
            crate::etiquettes::internal_error_chain::InternalErrorComplianceFinding,
        >,
    ) -> Self {
        Self {
            #[cfg(feature = "error_chain")]
            chain,
            #[cfg(feature = "internal_error_chain")]
            compliance,
        }
    }
}

/// Scan a shared compatibility tree for chain and compliance facts.
#[instrument(level = "debug", skip(syntax, file, layers))]
pub fn scan_rust_file_syntax(
    syntax: &syn::File,
    file: &Path,
    tree_root: &Path,
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

    visitor.into_file_scan()
}
