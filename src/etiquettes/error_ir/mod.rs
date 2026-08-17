//! Unified error IR source scan shared by error-handling etiquettes.
//!
//! `visitor` (error-sites logic) is unconditional. `chain_layer` and
//! `compliance_layer` are gated wholesale by a single `#[cfg]` on their
//! `mod` declaration below, rather than scattering `#[cfg(feature = ...)]`
//! across their internals.

mod visitor;

#[cfg(feature = "error_chain")]
mod chain_layer;
#[cfg(feature = "internal_error_chain")]
mod compliance_layer;

pub use visitor::{ErrorIrScanLayers, scan_rust_file_syntax};
