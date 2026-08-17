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
