//! Unified error-handling plugin for any workspace.

use std::sync::OnceLock;

use crate::etiquette::Etiquette;
use crate::plugin::{
    ErrorHandling, ErrorHandlingPolicy, ErrorScopeProvider, Plugin, PluginCategory,
    StandardErrorHandlingPolicy, WorkspaceMembersErrorScopeProvider,
};

use tracing::instrument;
static WORKSPACE_SCOPES: WorkspaceMembersErrorScopeProvider = WorkspaceMembersErrorScopeProvider;
static STANDARD_POLICY: StandardErrorHandlingPolicy = StandardErrorHandlingPolicy;

#[instrument(level = "debug")]
fn collect_error_handling_etiquettes() -> Vec<&'static dyn Etiquette> {
    let items: [Option<&'static dyn Etiquette>; 6] = [
        #[cfg(feature = "panics")]
        Some(&crate::etiquettes::panics::PANICS_ETIQUETTE),
        #[cfg(not(feature = "panics"))]
        None,
        #[cfg(feature = "error_sites")]
        Some(&crate::etiquettes::error_sites::ERROR_SITES_ETIQUETTE),
        #[cfg(not(feature = "error_sites"))]
        None,
        #[cfg(feature = "error_chain")]
        Some(&crate::etiquettes::error_chain::ERROR_CHAIN_ETIQUETTE),
        #[cfg(not(feature = "error_chain"))]
        None,
        #[cfg(feature = "internal_error_chain")]
        Some(&crate::etiquettes::internal_error_chain::INTERNAL_ERROR_CHAIN_ETIQUETTE),
        #[cfg(not(feature = "internal_error_chain"))]
        None,
        #[cfg(feature = "foreign_error_types")]
        Some(&crate::etiquettes::foreign_error_types::FOREIGN_ERROR_TYPES_ETIQUETTE),
        #[cfg(not(feature = "foreign_error_types"))]
        None,
        #[cfg(feature = "foreign_error_attenuation")]
        Some(&crate::etiquettes::foreign_error_attenuation::FOREIGN_ERROR_ATTENUATION_ETIQUETTE),
        #[cfg(not(feature = "foreign_error_attenuation"))]
        None,
    ];
    items.into_iter().flatten().collect()
}

#[instrument(level = "debug")]
fn leaked_error_handling_etiquette_slice() -> &'static [&'static dyn Etiquette] {
    static SLICE: OnceLock<&'static [&'static dyn Etiquette]> = OnceLock::new();
    SLICE.get_or_init(|| {
        let etiquettes = collect_error_handling_etiquettes();
        Box::leak(etiquettes.into_boxed_slice())
    })
}

/// Standard error-handling profile: panics plus the Result/chain stack
/// (feature-gated) on all workspace members.
#[derive(Debug, Default, Clone, Copy)]
pub struct StandardErrorHandling;

impl Plugin for StandardErrorHandling {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "error-handling"
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        "Error handling"
    }

    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> &[&'static dyn Etiquette] {
        leaked_error_handling_etiquette_slice()
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> PluginCategory {
        PluginCategory::ErrorHandling
    }
}

impl ErrorHandling for StandardErrorHandling {
    #[instrument(level = "trace", skip(self))]
    fn scope_provider(&self) -> &dyn ErrorScopeProvider {
        &WORKSPACE_SCOPES
    }

    #[instrument(level = "trace", skip(self))]
    fn policy(&self) -> &dyn ErrorHandlingPolicy {
        &STANDARD_POLICY
    }
}

/// Built-in unified error-handling plugin.
pub static STANDARD_ERROR_HANDLING: StandardErrorHandling = StandardErrorHandling;

/// Etiquette bundles contributed by [`STANDARD_ERROR_HANDLING`] for the current feature set.
#[instrument(level = "debug")]
pub fn standard_error_handling_etiquettes() -> Vec<&'static dyn Etiquette> {
    collect_error_handling_etiquettes()
}
