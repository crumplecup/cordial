//! Unified error IR source scan shared by error-handling etiquettes.
//!
//! **What.** One `syn` walk of a file produces the layers later etiquettes
//! read: sites always; chain preservation when `error_chain` is on;
//! internal compliance when `internal_error_chain` is on.
//!
//! **Why.** Sites, chain, internal graph, foreign types, and attenuation
//! must agree on the same rows. A shared scan avoids each etiquette
//! re-walking source with a slightly different visitor.
//!
//! **How to use.** Not a CLI etiquette. Enable `error_sites` (and optional
//! downstream features). `visitor` is unconditional. `chain_layer` and
//! `compliance_layer` are gated wholesale by a single `#[cfg]` on their
//! `mod` declaration, rather than scattering `#[cfg(feature = ...)]` across
//! their internals (`docs/planning/cfg-scatter-etiquette.md`).

mod visitor;

#[cfg(feature = "error_chain")]
mod chain_layer;
#[cfg(feature = "internal_error_chain")]
mod compliance_layer;

pub use visitor::{ErrorIrScanLayers, scan_rust_file_syntax};
