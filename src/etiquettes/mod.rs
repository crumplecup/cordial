#[cfg(feature = "allows")]
pub(crate) mod allows;
#[cfg(feature = "antipatterns")]
pub(crate) mod antipatterns;
#[cfg(feature = "cfg_scatter")]
pub(crate) mod cfg_scatter;
#[cfg(feature = "derives")]
pub(crate) mod derives;
#[cfg(feature = "error_sites")]
mod error_ir;
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
#[cfg(feature = "shadow")]
pub(crate) mod shadow;
#[cfg(feature = "tracing")]
pub(crate) mod tracing;
#[cfg(feature = "trenchcoat")]
pub(crate) mod trenchcoat;

/// Built-in source-quality etiquettes enabled in the current feature set.
pub fn quality_etiquettes() -> Vec<&'static dyn crate::Etiquette> {
    let items: [Option<&'static dyn crate::Etiquette>; 13] = [
        #[cfg(feature = "panics")]
        Some(&panics::PANICS_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "panics"))]
        None,
        #[cfg(feature = "tracing")]
        Some(&tracing::TRACING_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "tracing"))]
        None,
        #[cfg(feature = "allows")]
        Some(&allows::ALLOWS_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "allows"))]
        None,
        #[cfg(feature = "modularity")]
        Some(&modularity::MODULARITY_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "modularity"))]
        None,
        #[cfg(feature = "derives")]
        Some(&derives::DERIVES_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "derives"))]
        None,
        #[cfg(feature = "error_sites")]
        Some(&error_sites::ERROR_SITES_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "error_sites"))]
        None,
        #[cfg(feature = "error_chain")]
        Some(&error_chain::ERROR_CHAIN_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "error_chain"))]
        None,
        #[cfg(feature = "internal_error_chain")]
        Some(&internal_error_chain::INTERNAL_ERROR_CHAIN_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "internal_error_chain"))]
        None,
        #[cfg(feature = "foreign_error_types")]
        Some(&foreign_error_types::FOREIGN_ERROR_TYPES_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "foreign_error_types"))]
        None,
        #[cfg(feature = "foreign_error_attenuation")]
        Some(
            &foreign_error_attenuation::FOREIGN_ERROR_ATTENUATION_ETIQUETTE
                as &dyn crate::Etiquette,
        ),
        #[cfg(not(feature = "foreign_error_attenuation"))]
        None,
        #[cfg(feature = "antipatterns")]
        Some(&antipatterns::ANTIPATTERNS_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "antipatterns"))]
        None,
        #[cfg(feature = "cfg_scatter")]
        Some(&cfg_scatter::CFG_SCATTER_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "cfg_scatter"))]
        None,
        #[cfg(feature = "visibility")]
        Some(&visibility::VISIBILITY_ETIQUETTE as &dyn crate::Etiquette),
        #[cfg(not(feature = "visibility"))]
        None,
    ];
    items.into_iter().flatten().collect()
}

/// Built-in elicitation coverage etiquettes enabled in the current feature set.
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
