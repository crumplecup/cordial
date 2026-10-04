//! Owned etiquette handles.
//!
//! Built-in etiquettes are `static` values; etiquettes derived from
//! `cordial.toml` are owned at run time. Both travel as
//! `Arc<dyn Etiquette>` so the session does not care which it holds.

use std::sync::Arc;

use tracing::instrument;

use crate::hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};

use super::{Etiquette, EtiquetteExplain};

/// Anything that can be registered as an etiquette.
///
/// Implemented for owned handles and for `&'static` etiquettes, so
/// `register(&PAGEANTRY_ETIQUETTE)` and `register(Arc::new(custom))` both work.
pub trait IntoEtiquette {
    /// Convert into the shared handle the session stores.
    fn into_etiquette(self) -> Arc<dyn Etiquette>;
}

/// A borrowed etiquette is an etiquette.
///
/// This lets `&'static StaticEtiquette` (the built-in shape) become an
/// `Arc<dyn Etiquette>` without a wrapper type.
impl<T: Etiquette + ?Sized> Etiquette for &T {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        (**self).id()
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        (**self).name()
    }

    #[instrument(level = "trace", skip(self))]
    fn explain(&self) -> EtiquetteExplain {
        (**self).explain()
    }

    #[instrument(level = "trace", skip(self))]
    fn loaders(&self) -> &[&dyn Loader] {
        (**self).loaders()
    }

    #[instrument(level = "trace", skip(self))]
    fn enrichers(&self) -> &[&dyn IrEnricher] {
        (**self).enrichers()
    }

    #[instrument(level = "trace", skip(self))]
    fn probes(&self) -> &[&dyn Probe] {
        (**self).probes()
    }

    #[instrument(level = "trace", skip(self))]
    fn assessors(&self) -> &[&dyn Assessor] {
        (**self).assessors()
    }

    #[instrument(level = "trace", skip(self))]
    fn workspace_assessors(&self) -> &[&dyn WorkspaceAssessor] {
        (**self).workspace_assessors()
    }

    #[instrument(level = "trace", skip(self))]
    fn reporters(&self) -> &[&dyn Reporter] {
        (**self).reporters()
    }

    #[instrument(level = "trace", skip(self))]
    fn is_coverage(&self) -> bool {
        (**self).is_coverage()
    }
}

impl IntoEtiquette for Arc<dyn Etiquette> {
    #[instrument(level = "trace", skip(self))]
    fn into_etiquette(self) -> Arc<dyn Etiquette> {
        self
    }
}

impl<T: Etiquette + ?Sized + 'static> IntoEtiquette for &'static T {
    #[instrument(level = "trace", skip(self))]
    fn into_etiquette(self) -> Arc<dyn Etiquette> {
        Arc::new(self)
    }
}
