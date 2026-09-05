//! Unified error IR source scan shared by error-handling etiquettes.
//!
//! **What.** One `syn` walk of a file produces the layers consumed by the
//! error-handling family.
//!
//! **Why.** Sites, chain preservation, internal graph shape, foreign types,
//! and attenuation must agree on the same rows. A shared scan avoids each
//! etiquette re-walking source with a slightly different visitor.
//!
//! **Flags.** This module does not emit user-facing findings. It builds site
//! rows unconditionally for `error_sites`, chain-preservation rows when
//! `error_chain` is on, and internal-compliance rows when
//! `internal_error_chain` is on.
//!
//! **Ignores.** Judgment, exception handling, and artifact rendering stay in
//! the downstream etiquettes.
//!
//! **Outputs.** In-memory error IR layers attached to the shared graph.
//!
//! **Config.** Not a CLI etiquette. Enable `error_sites` and optional
//! downstream error features. `chain_layer` and `compliance_layer` are gated
//! wholesale at the module boundary instead of scattering feature cfgs across
//! their internals.

mod visitor;

#[cfg(feature = "error_chain")]
mod chain_layer;
#[cfg(feature = "internal_error_chain")]
mod compliance_layer;

pub use visitor::{ErrorIrScanLayers, scan_rust_file_syntax};
