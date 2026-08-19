//! Foreign types that reach elicitation traits unwrapped.
//!
//! **What.** From rustdoc JSON, finds types that implement (or should
//! implement) our traits while still exposing an unwrapped foreign type —
//! the “trenchcoat” is the wrapper that should sit in between.
//!
//! **Why.** Binding a foreign type directly to an elicitation trait couples
//! our surface to upstream layout and orphan-rule limits. Wrappers are the
//! seam that `impl-coverage` and `shadow` then measure.
//!
//! **How to use.** `cordial build rustdoc`, then `cordial coverage` (feature
//! `trenchcoat` / `elicitation`). Artifact: `{store}/findings/trenchcoats.csv`.
//! Register [`TRENCHCOAT_ETIQUETTE`].

mod assessor;
mod probe;
mod reporter;
mod types;

pub use assessor::TrenchcoatAssessor;
pub use probe::UnwrappedForeignProbe;
pub use reporter::TrenchcoatCsvReporter;

use crate::etiquette::StaticEtiquette;
use crate::{RustdocLoader, TrenchcoatEnricher};

static RUSTDOC_LOADER: RustdocLoader = RustdocLoader;
static TRENCHCOAT: TrenchcoatEnricher = TrenchcoatEnricher;
static UNWRAPPED_PROBE: UnwrappedForeignProbe = UnwrappedForeignProbe;
static TRENCHCOAT_ASSESSOR: TrenchcoatAssessor = TrenchcoatAssessor;
static TRENCHCOAT_CSV: TrenchcoatCsvReporter = TrenchcoatCsvReporter;

static LOADERS: &[&'static dyn crate::Loader] = &[&RUSTDOC_LOADER];
static ENRICHERS: &[&'static dyn crate::IrEnricher] = &[&TRENCHCOAT];
static PROBES: &[&'static dyn crate::Probe] = &[&UNWRAPPED_PROBE];
static ASSESSORS: &[&'static dyn crate::Assessor] = &[&TRENCHCOAT_ASSESSOR];
static REPORTERS: &[&'static dyn crate::Reporter] = &[&TRENCHCOAT_CSV];

/// Built-in trenchcoat wrapper coverage etiquette bundle.
pub static TRENCHCOAT_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "trenchcoat",
    name: "Trenchcoat wrappers",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: true,
};
