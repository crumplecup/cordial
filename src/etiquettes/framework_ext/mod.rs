//! Amenable ext (third-party crate) registry coverage — the
//! `amenable_ext` counterpart of `etiquettes::framework_std`'s
//! `amenable-std` bundle.
//!
//! **What.** Workspace-scoped coverage etiquette for jiff's registered
//! carriers: how much of jiff's inventory has `ExtStandard<T>` evidence
//! and verifier witnesses in `amenable_ext`.
//!
//! **Why.** Same denominator question `amenable-std` asks, one target
//! crate at a time via a shadow-dep rustdoc build instead of the shared
//! sysroot cache.
//!
//! **Flags.** `amenable-ext-jiff` inventories jiff specifically; a
//! second target (e.g. chrono) would get its own sibling module and
//! etiquette here, not a rename of this one.
//!
//! **Ignores.** Source-quality findings, local wrapper gaps, and project
//! trait coverage belong to other etiquettes.
//!
//! **Outputs.** `{store}/findings/amenable-ext-jiff.checklist.md` and
//! jiff coverage inventory artifacts.
//!
//! **Config.** Run `cordial coverage`. Requires the `amenable_ext`
//! feature (which requires `amenable_std` + `shadow` — see
//! `docs/AMENABLE_EXT_COVERAGE.md` in the `amenable` repo for the full
//! design). Builds jiff's rustdoc JSON via a shadow-dep build the first
//! time it's needed, then reuses the cache.

mod assessor;
mod jiff;
mod probe;
mod reporter;

pub const AMENABLE_EXT_JIFF_CATEGORY: &str = "amenable-ext-jiff";

pub use assessor::AmenableExtJiffAssessor;
pub use jiff::amenable_ext_jiff_report_from_findings;
pub use probe::AmenableExtJiffScopeProbe;
pub use reporter::AmenableExtJiffReporter;

use crate::etiquette::{EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, StaticEtiquette};

static AMENABLE_EXT_JIFF_PROBE: AmenableExtJiffScopeProbe = AmenableExtJiffScopeProbe;
static AMENABLE_EXT_JIFF_ASSESSOR: AmenableExtJiffAssessor = AmenableExtJiffAssessor;
static AMENABLE_EXT_JIFF_REPORTER: AmenableExtJiffReporter = AmenableExtJiffReporter;

static AMENABLE_EXT_JIFF_PROBES: &[&'static dyn crate::Probe] = &[&AMENABLE_EXT_JIFF_PROBE];
static AMENABLE_EXT_JIFF_ASSESSORS: &[&'static dyn crate::Assessor] =
    &[&AMENABLE_EXT_JIFF_ASSESSOR];

/// Workspace-scoped amenable ext (jiff) registry coverage reporter.
pub static AMENABLE_EXT_JIFF_ETIQUETTE: StaticEtiquette = StaticEtiquette::new(
    "amenable-ext-jiff",
    "Amenable ext (jiff) coverage",
    EtiquetteHooks::new(
        &[],
        &[],
        AMENABLE_EXT_JIFF_PROBES,
        AMENABLE_EXT_JIFF_ASSESSORS,
        None,
        &[&AMENABLE_EXT_JIFF_REPORTER],
    ),
    true,
    EtiquetteExplain::new(
        "How much of jiff's registered carrier surface is in the amenable_ext registry?",
        "Same registry-coverage question amenable-std asks, for a third-party target crate (jiff) via a shadow-dep rustdoc build instead of the shared sysroot cache.",
        "Workspace-scoped; no source loaders. Builds/reuses jiff's shadow-dep rustdoc cache. Feature amenable_ext.",
        "`[amenable-ext-jiff] enabled = false` in cordial.toml.",
        &[EtiquetteRuleExplain::new(
            "AMENABLE-EXT-JIFF-ROW",
            "Jiff inventory row assessed for amenable_ext registry coverage",
        )],
    ),
);
