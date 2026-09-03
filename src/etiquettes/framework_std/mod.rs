//! Std-family coverage for homecoming `Code` and the amenable registry.
//!
//! **What.** Two workspace-scoped etiquettes, not source scanners:
//!
//! - `homecoming-std` — how much of `std` / `core` / `alloc` implements
//!   homecoming `Code` ([`HOMECOMING_STD_ETIQUETTE`]).
//! - `amenable-std` — how much of that surface is in the amenable registry
//!   (`AMENABLE_STD_ETIQUETTE`, feature `amenable_std`).
//!
//! **Why.** Framework coverage is a different denominator from project
//! elicitation: the question is “which std types are first-class in this
//! ecosystem,” not “did this workspace crate wrap its foreign types.”
//!
//! **How to use.** `cordial build sysroot` (needs `homecoming_std`), then
//! `cordial coverage`. Artifact: `{store}/findings/std.checklist.md`. These
//! bundles have no source loaders; they consume sysroot rustdoc already in
//! the store.

#[cfg(feature = "amenable_std")]
mod amenable;
#[cfg(feature = "amenable_std")]
mod amenable_reporter;
mod assessor;
mod homecoming;
mod probe;
mod reporter;

pub const HOMECOMING_STD_CATEGORY: &str = "homecoming-std";
pub const AMENABLE_STD_CATEGORY: &str = "amenable-std";

#[cfg(feature = "amenable_std")]
pub use self::{
    amenable::amenable_report_from_findings, amenable_reporter::AmenableStdReporter,
    assessor::AmenableStdAssessor, probe::AmenableStdScopeProbe,
};
pub use assessor::HomecomingStdAssessor;
pub use homecoming::framework_report_from_findings;
pub use probe::HomecomingStdScopeProbe;
pub use reporter::HomecomingStdReporter;

use crate::etiquette::{EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette};

static HOMECOMING_STD_PROBE: HomecomingStdScopeProbe = HomecomingStdScopeProbe;
static HOMECOMING_STD_ASSESSOR: HomecomingStdAssessor = HomecomingStdAssessor;
static HOMECOMING_STD_REPORTER: HomecomingStdReporter = HomecomingStdReporter;

static HOMECOMING_PROBES: &[&'static dyn crate::Probe] = &[&HOMECOMING_STD_PROBE];
static HOMECOMING_ASSESSORS: &[&'static dyn crate::Assessor] = &[&HOMECOMING_STD_ASSESSOR];

/// Workspace-scoped framework std coverage (homecoming `Code` reporter).
pub static HOMECOMING_STD_ETIQUETTE: StaticEtiquette = StaticEtiquette::new(
    "homecoming-std",
    "Homecoming std coverage",
    EtiquetteHooks::new(
        &[],
        &[],
        HOMECOMING_PROBES,
        HOMECOMING_ASSESSORS,
        None,
        &[&HOMECOMING_STD_REPORTER],
    ),
    true,
    EtiquetteExplain::new(
        "How much of Rust std / core / alloc implements homecoming Code?",
        "Framework coverage is a different denominator from project elicitation: which std types are first-class in this ecosystem, not did this workspace crate wrap its foreign types.",
        "Workspace-scoped; no source loaders. Consumes sysroot rustdoc already in the store (cordial build sysroot). Artifact: std.checklist.md.",
        "`[homecoming-std] enabled = false` in cordial.toml.",
        &[EtiquetteRuleExplain::new(
            "FRAMEWORK-STD-ROW",
            "Std inventory row assessed for Code coverage",
        )],
    ),
);

/// Workspace-scoped amenable std registry coverage reporter, gated as a
/// whole unit — see `docs/planning/cfg-scatter-etiquette.md` for the pattern.
#[cfg(feature = "amenable_std")]
mod amenable_etiquette {
    use super::{AmenableStdAssessor, AmenableStdReporter, AmenableStdScopeProbe};
    use crate::etiquette::{
        EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette,
    };

    static AMENABLE_STD_PROBE: AmenableStdScopeProbe = AmenableStdScopeProbe;
    static AMENABLE_STD_ASSESSOR: AmenableStdAssessor = AmenableStdAssessor;
    static AMENABLE_STD_REPORTER: AmenableStdReporter = AmenableStdReporter;

    static AMENABLE_PROBES: &[&'static dyn crate::Probe] = &[&AMENABLE_STD_PROBE];
    static AMENABLE_ASSESSORS: &[&'static dyn crate::Assessor] = &[&AMENABLE_STD_ASSESSOR];

    /// `AMENABLE_STD_ETIQUETTE`.
    pub static AMENABLE_STD_ETIQUETTE: StaticEtiquette = StaticEtiquette::new(
        "amenable-std",
        "Amenable std coverage",
        EtiquetteHooks::new(
            &[],
            &[],
            AMENABLE_PROBES,
            AMENABLE_ASSESSORS,
            None,
            &[&AMENABLE_STD_REPORTER],
        ),
        true,
        EtiquetteExplain::new(
            "How much of that std surface is in the amenable registry?",
            "Same std-family denominator as homecoming-std; this page is the registry coverage half.",
            "Workspace-scoped; no source loaders. Consumes sysroot rustdoc already in the store. Feature amenable_std.",
            "`[amenable-std] enabled = false` in cordial.toml.",
            &[EtiquetteRuleExplain::new(
                "AMENABLE-STD-ROW",
                "Std inventory row assessed for amenable registry coverage",
            )],
        ),
    );
}

#[cfg(feature = "amenable_std")]
pub use amenable_etiquette::AMENABLE_STD_ETIQUETTE;
