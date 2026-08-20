//! Error-handling family: a named product type that implements [`ErrorHandling`].
//!
//! Narrower than cordial's [`STANDARD_ERROR_HANDLING`]: sites + chain only.

use cordial::{
    ERROR_CHAIN_ETIQUETTE, ERROR_SITES_ETIQUETTE, ErrorHandling, ErrorHandlingLayers,
    ErrorHandlingPolicy, ErrorScopeProvider, Etiquette, Plugin, PluginCategory,
    WorkspaceMembersErrorScopeProvider,
};

use tracing::instrument;
static WORKSPACE_SCOPES: WorkspaceMembersErrorScopeProvider = WorkspaceMembersErrorScopeProvider;
static ACME_POLICY: AcmeErrorPolicy = AcmeErrorPolicy;

static ACME_ERROR_ETIQUETTES: &[&dyn Etiquette] = &[&ERROR_SITES_ETIQUETTE, &ERROR_CHAIN_ETIQUETTE];

/// Sites and chain preservation only — no internal / foreign / attenuation.
#[derive(Debug, Default, Clone, Copy)]
pub struct AcmeErrorPolicy;

impl ErrorHandlingPolicy for AcmeErrorPolicy {
    #[instrument(level = "trace", skip(self))]
    fn layers(&self) -> ErrorHandlingLayers {
        ErrorHandlingLayers {
            panics: false,
            sites: true,
            chain: true,
            internal: false,
            foreign_types: false,
            attenuation: false,
        }
    }
}

/// Acme error-handling — `ErrorHandling: Plugin` on all workspace members.
#[derive(Debug, Default, Clone, Copy)]
pub struct AcmeErrorHandling;

impl Plugin for AcmeErrorHandling {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "acme-error-handling"
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        "Acme error handling"
    }

    #[instrument(level = "trace", skip(self))]
    fn etiquettes(&self) -> &[&'static dyn Etiquette] {
        ACME_ERROR_ETIQUETTES
    }

    #[instrument(level = "trace", skip(self))]
    fn category(&self) -> PluginCategory {
        PluginCategory::ErrorHandling
    }
}

impl ErrorHandling for AcmeErrorHandling {
    #[instrument(level = "trace", skip(self))]
    fn scope_provider(&self) -> &dyn ErrorScopeProvider {
        &WORKSPACE_SCOPES
    }

    #[instrument(level = "trace", skip(self))]
    fn policy(&self) -> &dyn ErrorHandlingPolicy {
        &ACME_POLICY
    }
}

/// Built-in instance for session registration.
pub static ACME_ERROR_HANDLING: AcmeErrorHandling = AcmeErrorHandling;
