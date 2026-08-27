//! Built-in etiquettes: quality scanners and coverage inventories.
//!
//! An etiquette is one polite standard: a named bundle of loaders, enrichers,
//! probes, assessors, and reporters. Quality etiquettes walk source into the
//! graph IR. Coverage etiquettes need rustdoc JSON (`cordial build rustdoc`).
//!
//! Run `cordial quality` or `cordial coverage`, or register a bundle on a
//! [`crate::Session`]. Thresholds load from `cordial.toml` (workspace, then
//! store home). Exceptions are JSON patches under the project store
//! (`cordial exceptions`).
//!
//! Each child module’s docs cover what it checks, why, and how to use it.
//! Policy tables live in `docs/planning/`.
//!
//! # Quality
//!
//! | Id | Module | Question |
//! | --- | --- | --- |
//! | `panics` | `panics` | Where does this crate abort? |
//! | `tracing` | `tracing` | Are functions instrumented with the recipe for their role, and is a subscriber installed? |
//! | `allows` | `allows` | Which `#[allow]` attributes are in force? |
//! | `modularity` | `modularity` | Which files, functions, and modules are too large, overpacked, top-heavy, lopsided, or a unary nest? |
//! | `derives` | `derives` | Which manual builders/getters/setters/`new`/pub fields could be derives? |
//! | `error_sites` | `error_sites` | Where are `?`, `map_err`, and related error sites? |
//! | `error_chain` | `error_chain` | Which sites drop the source error instead of chaining it? |
//! | `internal_error_chain` | `internal_error_chain` | Parent / Kind / native-source architecture? |
//! | `foreign_error_types` | `foreign_error_types` | Which foreign error types leak into this crate? |
//! | `foreign_error_attenuation` | `foreign_error_attenuation` | How should those foreign errors be wrapped? |
//! | `antipatterns` | `antipatterns` | Untyped error carriers and related smells |
//! | `cfg_scatter` | `cfg_scatter` | Is the same `#[cfg]` copied across item kinds? |
//! | `cfg_hygiene` | `cfg_hygiene` | Is every `cfg` name declared, and does each verifier crate only use its own? |
//! | `visibility` | `visibility` | Do `pub mod` paths earn their existence? |
//! | `cli_layout` | `cli_layout` | Do clap types dispatch in the library with `act`? |
//! | `glob_imports` | `glob_imports` | Are there glob `use` trees (`foo::*`, including `super::*`)? |
//! | `inline_tests` | `inline_tests` | Are tests mixed into `src/` instead of `tests/`? |
//! | `verus_warnings` | `verus_warnings` | Does the Verus rustc fork emit warnings this crate's rustc never sees? |
//! | `proof_patterns` | `proof_patterns` | Which `verus!` functions are trusted rather than proven, or apply themselves invisibly (`broadcast`)? |
//!
//! # Coverage
//!
//! | Id | Module | Question |
//! | --- | --- | --- |
//! | `impl-coverage` | `impl_coverage` | Do types implement the required elicitation traits? |
//! | `trenchcoat` | `trenchcoat` | Are foreign types wrapped before they reach our traits? |
//! | `shadow` | `shadow` | Do shadow crates mirror upstream items? |
//! | `homecoming-std` | `framework_std` | How much of Rust `std` implements homecoming `Code`? |
//! | `amenable-std` | `framework_std` | How much of `std` is in the amenable registry? |
//!
//! Error-handling etiquettes share `error_ir`. Coverage etiquettes need
//! rustdoc JSON.

#[cfg(feature = "allows")]
pub(crate) mod allows;
#[cfg(feature = "antipatterns")]
pub(crate) mod antipatterns;
#[cfg(feature = "cfg_hygiene")]
pub(crate) mod cfg_hygiene;
#[cfg(feature = "cfg_scatter")]
pub(crate) mod cfg_scatter;
#[cfg(feature = "cli_layout")]
pub(crate) mod cli_layout;
#[cfg(feature = "derives")]
pub(crate) mod derives;
#[cfg(feature = "error_sites")]
mod error_ir;
#[cfg(feature = "glob_imports")]
pub(crate) mod glob_imports;
#[cfg(feature = "inline_tests")]
pub(crate) mod inline_tests;
#[cfg(feature = "verus_warnings")]
pub(crate) mod verus_warnings;
#[cfg(feature = "visibility")]
pub(crate) mod visibility;
#[cfg(feature = "error_sites")]
pub(crate) use error_ir::{ErrorIrScanLayers, scan_rust_file_syntax};
#[cfg(feature = "error_chain")]
pub(crate) mod error_chain;
#[cfg(feature = "error_sites")]
pub(crate) mod error_sites;
#[cfg(feature = "foreign_error_attenuation")]
pub(crate) mod foreign_error_attenuation;
#[cfg(feature = "foreign_error_types")]
pub(crate) mod foreign_error_types;
#[cfg(feature = "homecoming_std")]
pub(crate) mod framework_std;
#[cfg(feature = "impl_coverage")]
pub(crate) mod impl_coverage;
#[cfg(feature = "internal_error_chain")]
pub(crate) mod internal_error_chain;
#[cfg(feature = "modularity")]
pub(crate) mod modularity;
#[cfg(feature = "panics")]
pub(crate) mod panics;
#[cfg(feature = "proof_patterns")]
pub(crate) mod proof_patterns;
#[cfg(feature = "shadow")]
pub(crate) mod shadow;
#[cfg(feature = "tracing")]
pub(crate) mod tracing;
#[cfg(feature = "trenchcoat")]
pub(crate) mod trenchcoat;

/// Every built-in quality etiquette in the current feature set, as the
/// combined `Etiquette + QualityReportArea` supertrait -- the one
/// canonical list both [`quality_etiquettes`] (session/plugin
/// registration) and [`quality_report_areas`] (the `quality-report.md`
/// rollup) derive from. An etiquette module compiles into this list only
/// once it's a [`crate::etiquette::StaticQualityEtiquette`] with its
/// `quality_area` field set (`Some(..)` or an explicit, documented
/// `None`) -- there is no path to appear in `quality_etiquettes()` while
/// silently missing from the rollup, or vice versa (see
/// `docs/planning/quality-report-feeder-trait.md`).
#[::tracing::instrument(level = "debug")]
fn quality_report_etiquettes() -> Vec<&'static dyn crate::etiquette::QualityEtiquette> {
    let items: [Option<&'static dyn crate::etiquette::QualityEtiquette>; 19] = [
        #[cfg(feature = "panics")]
        Some(&panics::PANICS_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "panics"))]
        None,
        #[cfg(feature = "tracing")]
        Some(&tracing::TRACING_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "tracing"))]
        None,
        #[cfg(feature = "allows")]
        Some(&allows::ALLOWS_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "allows"))]
        None,
        #[cfg(feature = "modularity")]
        Some(&modularity::MODULARITY_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "modularity"))]
        None,
        #[cfg(feature = "derives")]
        Some(&derives::DERIVES_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "derives"))]
        None,
        #[cfg(feature = "error_sites")]
        Some(&error_sites::ERROR_SITES_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "error_sites"))]
        None,
        #[cfg(feature = "error_chain")]
        Some(&error_chain::ERROR_CHAIN_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "error_chain"))]
        None,
        #[cfg(feature = "internal_error_chain")]
        Some(
            &internal_error_chain::INTERNAL_ERROR_CHAIN_ETIQUETTE
                as &dyn crate::etiquette::QualityEtiquette,
        ),
        #[cfg(not(feature = "internal_error_chain"))]
        None,
        #[cfg(feature = "foreign_error_types")]
        Some(
            &foreign_error_types::FOREIGN_ERROR_TYPES_ETIQUETTE
                as &dyn crate::etiquette::QualityEtiquette,
        ),
        #[cfg(not(feature = "foreign_error_types"))]
        None,
        #[cfg(feature = "foreign_error_attenuation")]
        Some(
            &foreign_error_attenuation::FOREIGN_ERROR_ATTENUATION_ETIQUETTE
                as &dyn crate::etiquette::QualityEtiquette,
        ),
        #[cfg(not(feature = "foreign_error_attenuation"))]
        None,
        #[cfg(feature = "antipatterns")]
        Some(&antipatterns::ANTIPATTERNS_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "antipatterns"))]
        None,
        #[cfg(feature = "cfg_scatter")]
        Some(&cfg_scatter::CFG_SCATTER_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "cfg_scatter"))]
        None,
        #[cfg(feature = "cfg_hygiene")]
        Some(&cfg_hygiene::CFG_HYGIENE_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "cfg_hygiene"))]
        None,
        #[cfg(feature = "visibility")]
        Some(&visibility::VISIBILITY_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "visibility"))]
        None,
        #[cfg(feature = "cli_layout")]
        Some(&cli_layout::CLI_LAYOUT_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "cli_layout"))]
        None,
        #[cfg(feature = "glob_imports")]
        Some(&glob_imports::GLOB_IMPORTS_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "glob_imports"))]
        None,
        #[cfg(feature = "inline_tests")]
        Some(&inline_tests::INLINE_TESTS_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "inline_tests"))]
        None,
        #[cfg(feature = "verus_warnings")]
        Some(&verus_warnings::VERUS_WARNINGS_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "verus_warnings"))]
        None,
        #[cfg(feature = "proof_patterns")]
        Some(&proof_patterns::PROOF_PATTERNS_ETIQUETTE as &dyn crate::etiquette::QualityEtiquette),
        #[cfg(not(feature = "proof_patterns"))]
        None,
    ];
    items.into_iter().flatten().collect()
}

/// Built-in source-quality etiquettes enabled in the current feature set.
#[::tracing::instrument(level = "debug")]
pub fn quality_etiquettes() -> Vec<&'static dyn crate::Etiquette> {
    quality_report_etiquettes()
        .into_iter()
        .map(|etiquette| etiquette as &dyn crate::Etiquette)
        .collect()
}

/// Every quality etiquette's own `quality-report.md` rollup contribution
/// (`None` for one that declines, on purpose) -- the source
/// [`crate::reporter::build_quality_report`] iterates instead of a
/// separately hand-maintained area list.
#[::tracing::instrument(level = "debug")]
pub(crate) fn quality_report_areas() -> Vec<&'static dyn crate::etiquette::QualityReportArea> {
    quality_report_etiquettes()
        .into_iter()
        .map(|etiquette| etiquette as &dyn crate::etiquette::QualityReportArea)
        .collect()
}

/// Built-in elicitation coverage etiquettes enabled in the current feature set.
#[::tracing::instrument(level = "debug")]
#[cfg(feature = "elicitation")]
pub fn coverage_etiquettes() -> Vec<&'static dyn crate::Etiquette> {
    let items: [Option<&'static dyn crate::Etiquette>; 3] = [
        #[cfg(feature = "impl_coverage")]
        Some(&impl_coverage::IMPL_COVERAGE_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "impl_coverage"))]
        None,
        #[cfg(feature = "trenchcoat")]
        Some(&trenchcoat::TRENCHCOAT_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "trenchcoat"))]
        None,
        #[cfg(feature = "shadow")]
        Some(&shadow::SHADOW_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "shadow"))]
        None,
    ];
    items.into_iter().flatten().collect()
}
