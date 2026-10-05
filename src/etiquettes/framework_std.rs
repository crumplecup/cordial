//! Std-family coverage for homecoming `Code` and the amenable registry.
//!
//! **What.** Provides workspace-scoped coverage etiquettes for standard
//! library ecosystem surfaces, not source scanners.
//!
//! **Why.** Framework coverage has a different denominator from project
//! elicitation: the question is which std-family types are first-class in the
//! ecosystem, not whether a workspace crate wrapped its foreign types.
//!
//! **Flags.** `homecoming-std` inventories how much of `std` / `core` /
//! `alloc` implements homecoming `Code`. `amenable-std` inventories how much
//! of that surface is in the amenable registry.
//!
//! **Ignores.** Source-quality findings, local wrapper gaps, and project
//! trait coverage belong to other etiquettes.
//!
//! **Outputs.** `{store}/findings/std.checklist.md` and std coverage
//! inventory artifacts.
//!
//! **Config.** Run `cordial build sysroot`, then `cordial coverage`.
//! `amenable-std` requires the `amenable_std` feature. These bundles consume
//! sysroot rustdoc already in the store.

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

use std::sync::LazyLock;

use crate::etiquette::{EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette};

static HOMECOMING_STD_PROBE: HomecomingStdScopeProbe = HomecomingStdScopeProbe;
static HOMECOMING_STD_ASSESSOR: HomecomingStdAssessor = HomecomingStdAssessor;
static HOMECOMING_STD_REPORTER: HomecomingStdReporter = HomecomingStdReporter;

static HOMECOMING_PROBES: &[&'static dyn crate::Probe] = &[&HOMECOMING_STD_PROBE];
static HOMECOMING_ASSESSORS: &[&'static dyn crate::Assessor] = &[&HOMECOMING_STD_ASSESSOR];
static HOMECOMING_LOADERS: &[&'static dyn crate::Loader] = &[];
static HOMECOMING_ENRICHERS: &[&'static dyn crate::IrEnricher] = &[];
static HOMECOMING_REPORTERS: &[&'static dyn crate::Reporter] = &[&HOMECOMING_STD_REPORTER];

/// Workspace-scoped framework std coverage (homecoming `Code` reporter).
pub static HOMECOMING_STD_ETIQUETTE: LazyLock<StaticEtiquette> = LazyLock::new(|| {
    StaticEtiquette::new(
        "homecoming-std",
        "Homecoming std coverage",
        EtiquetteHooks::new(
            HOMECOMING_LOADERS,
            HOMECOMING_ENRICHERS,
            HOMECOMING_PROBES,
            HOMECOMING_ASSESSORS,
            None,
            HOMECOMING_REPORTERS,
        ),
        true,
        EtiquetteExplain::new(
            "How much of Rust std / core / alloc implements homecoming Code?",
            "Framework coverage is a different denominator from project elicitation: which std types are first-class in this ecosystem, not did this workspace crate wrap its foreign types.",
            "Workspace-scoped; no source loaders. Consumes sysroot rustdoc already in the store (cordial build sysroot). Artifact: std.checklist.md.",
            "`[homecoming-std] enabled = false` in cordial.toml.",
            vec![EtiquetteRuleExplain::new(
                "FRAMEWORK-STD-ROW",
                "Std inventory row assessed for Code coverage",
            )],
        ),
    )
});

/// Workspace-scoped amenable std registry coverage reporter, gated as a
/// whole unit — see `docs/planning/cfg-scatter-etiquette.md` for the pattern.
#[cfg(feature = "amenable_std")]
mod amenable_etiquette {
    use std::sync::LazyLock;

    use super::{AmenableStdAssessor, AmenableStdReporter, AmenableStdScopeProbe};
    use crate::etiquette::{
        EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette,
    };

    static AMENABLE_STD_PROBE: AmenableStdScopeProbe = AmenableStdScopeProbe;
    static AMENABLE_STD_ASSESSOR: AmenableStdAssessor = AmenableStdAssessor;
    static AMENABLE_STD_REPORTER: AmenableStdReporter = AmenableStdReporter;

    static AMENABLE_PROBES: &[&'static dyn crate::Probe] = &[&AMENABLE_STD_PROBE];
    static AMENABLE_ASSESSORS: &[&'static dyn crate::Assessor] = &[&AMENABLE_STD_ASSESSOR];
    static AMENABLE_LOADERS: &[&'static dyn crate::Loader] = &[];
    static AMENABLE_ENRICHERS: &[&'static dyn crate::IrEnricher] = &[];
    static AMENABLE_REPORTERS: &[&'static dyn crate::Reporter] = &[&AMENABLE_STD_REPORTER];

    /// `AMENABLE_STD_ETIQUETTE`.
    pub static AMENABLE_STD_ETIQUETTE: LazyLock<StaticEtiquette> = LazyLock::new(|| {
        StaticEtiquette::new(
            "amenable-std",
            "Amenable std coverage",
            EtiquetteHooks::new(
                AMENABLE_LOADERS,
                AMENABLE_ENRICHERS,
                AMENABLE_PROBES,
                AMENABLE_ASSESSORS,
                None,
                AMENABLE_REPORTERS,
            ),
            true,
            EtiquetteExplain::new(
                "How much of that std surface is in the amenable registry?",
                "Same std-family denominator as homecoming-std; this page is the registry coverage half.",
                "Workspace-scoped; no source loaders. Consumes sysroot rustdoc already in the store. Feature amenable_std.",
                "`[amenable-std] enabled = false` in cordial.toml.",
                vec![EtiquetteRuleExplain::new(
                    "AMENABLE-STD-ROW",
                    "Std inventory row assessed for amenable registry coverage",
                )],
            ),
        )
    });
}

#[cfg(feature = "amenable_std")]
pub use amenable_etiquette::AMENABLE_STD_ETIQUETTE;
