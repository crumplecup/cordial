//! Hook-slice table for static etiquette declarations.

use crate::hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};

/// The hook slices an etiquette contributes, grouped so
/// [`crate::etiquette::StaticEtiquette`] binds them as one argument instead of
/// six.
pub struct EtiquetteHooks {
    loaders: &'static [&'static dyn Loader],
    enrichers: &'static [&'static dyn IrEnricher],
    probes: &'static [&'static dyn Probe],
    assessors: &'static [&'static dyn Assessor],
    workspace_assessors: Option<&'static [&'static dyn WorkspaceAssessor]>,
    reporters: &'static [&'static dyn Reporter],
}

impl EtiquetteHooks {
    /// Bind the hook slices for an etiquette table.
    ///
    /// Empty slices are meaningful: they say this etiquette does not
    /// participate in that pipeline phase.
    pub const fn new(
        loaders: &'static [&'static dyn Loader],
        enrichers: &'static [&'static dyn IrEnricher],
        probes: &'static [&'static dyn Probe],
        assessors: &'static [&'static dyn Assessor],
        workspace_assessors: Option<&'static [&'static dyn WorkspaceAssessor]>,
        reporters: &'static [&'static dyn Reporter],
    ) -> Self {
        Self {
            loaders,
            enrichers,
            probes,
            assessors,
            workspace_assessors,
            reporters,
        }
    }

    pub(in crate::etiquette) const fn loaders(&self) -> &'static [&'static dyn Loader] {
        self.loaders
    }

    pub(in crate::etiquette) const fn enrichers(&self) -> &'static [&'static dyn IrEnricher] {
        self.enrichers
    }

    pub(in crate::etiquette) const fn probes(&self) -> &'static [&'static dyn Probe] {
        self.probes
    }

    pub(in crate::etiquette) const fn assessors(&self) -> &'static [&'static dyn Assessor] {
        self.assessors
    }

    pub(in crate::etiquette) const fn workspace_assessors(
        &self,
    ) -> Option<&'static [&'static dyn WorkspaceAssessor]> {
        self.workspace_assessors
    }

    pub(in crate::etiquette) const fn reporters(&self) -> &'static [&'static dyn Reporter] {
        self.reporters
    }
}
