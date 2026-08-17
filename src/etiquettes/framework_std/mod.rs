#[cfg(feature = "amenable_std")]
mod amenable_reporter;
mod assessor;
mod probe;
mod reporter;
mod types;

#[cfg(feature = "amenable_std")]
pub use self::{
    amenable_reporter::AmenableStdReporter, assessor::AmenableStdAssessor,
    probe::AmenableStdScopeProbe, types::amenable_report_from_findings,
};
pub use assessor::HomecomingStdAssessor;
pub use probe::HomecomingStdScopeProbe;
pub use reporter::HomecomingStdReporter;
pub use types::framework_report_from_findings;

use crate::etiquette::StaticEtiquette;

static HOMECOMING_STD_PROBE: HomecomingStdScopeProbe = HomecomingStdScopeProbe;
static HOMECOMING_STD_ASSESSOR: HomecomingStdAssessor = HomecomingStdAssessor;
static HOMECOMING_STD_REPORTER: HomecomingStdReporter = HomecomingStdReporter;

static HOMECOMING_PROBES: &[&'static dyn crate::Probe] = &[&HOMECOMING_STD_PROBE];
static HOMECOMING_ASSESSORS: &[&'static dyn crate::Assessor] = &[&HOMECOMING_STD_ASSESSOR];

/// Workspace-scoped framework std coverage (homecoming `Code` reporter).
pub static HOMECOMING_STD_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "homecoming-std",
    name: "Homecoming std coverage",
    loaders: &[],
    enrichers: &[],
    probes: HOMECOMING_PROBES,
    assessors: HOMECOMING_ASSESSORS,
    workspace_assessors: None,
    reporters: &[&HOMECOMING_STD_REPORTER],
    is_coverage: true,
};

/// Workspace-scoped amenable std registry coverage reporter, gated as a
/// whole unit — see `docs/planning/cfg-scatter-etiquette.md` for the pattern.
#[cfg(feature = "amenable_std")]
mod amenable {
    use super::*;

    static AMENABLE_STD_PROBE: AmenableStdScopeProbe = AmenableStdScopeProbe;
    static AMENABLE_STD_ASSESSOR: AmenableStdAssessor = AmenableStdAssessor;
    static AMENABLE_STD_REPORTER: AmenableStdReporter = AmenableStdReporter;

    static AMENABLE_PROBES: &[&'static dyn crate::Probe] = &[&AMENABLE_STD_PROBE];
    static AMENABLE_ASSESSORS: &[&'static dyn crate::Assessor] = &[&AMENABLE_STD_ASSESSOR];

    pub static AMENABLE_STD_ETIQUETTE: StaticEtiquette = StaticEtiquette {
        id: "amenable-std",
        name: "Amenable std coverage",
        loaders: &[],
        enrichers: &[],
        probes: AMENABLE_PROBES,
        assessors: AMENABLE_ASSESSORS,
        workspace_assessors: None,
        reporters: &[&AMENABLE_STD_REPORTER],
        is_coverage: true,
    };
}

#[cfg(feature = "amenable_std")]
pub use amenable::AMENABLE_STD_ETIQUETTE;
