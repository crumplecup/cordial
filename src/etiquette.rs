//! An **etiquette** is a named bundle of analysis hooks: loaders, enrichers,
//! probes, assessors, and reporters.
//!
//! Built-in etiquettes live under `src/etiquettes/`. Register one on a
//! [`crate::Session`] with [`crate::Session::register`], or run the CLI
//! (`cordial quality`, `cordial coverage`).

use crate::hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};

use tracing::instrument;
/// Named bundle of cordial hook implementations.
pub trait Etiquette: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;

    fn loaders(&self) -> &[&dyn Loader];
    fn enrichers(&self) -> &[&dyn IrEnricher];
    fn probes(&self) -> &[&dyn Probe];
    fn assessors(&self) -> &[&dyn Assessor];
    fn workspace_assessors(&self) -> &[&dyn WorkspaceAssessor] {
        &[]
    }
    fn reporters(&self) -> &[&dyn Reporter];

    /// True for trait-impl / framework coverage hook bundles (not source-quality scans).
    fn is_coverage(&self) -> bool {
        false
    }
}

/// Static etiquette declaration backed by slices of trait object references.
#[derive(Default)]
pub struct StaticEtiquette {
    pub id: &'static str,
    pub name: &'static str,
    pub loaders: &'static [&'static dyn Loader],
    pub enrichers: &'static [&'static dyn IrEnricher],
    pub probes: &'static [&'static dyn Probe],
    pub assessors: &'static [&'static dyn Assessor],
    pub workspace_assessors: Option<&'static [&'static dyn WorkspaceAssessor]>,
    pub reporters: &'static [&'static dyn Reporter],
    pub is_coverage: bool,
}

impl Etiquette for StaticEtiquette {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.id
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        self.name
    }

    #[instrument(level = "trace", skip(self))]
    fn loaders(&self) -> &[&dyn Loader] {
        self.loaders
    }

    #[instrument(level = "trace", skip(self))]
    fn enrichers(&self) -> &[&dyn IrEnricher] {
        self.enrichers
    }

    #[instrument(level = "trace", skip(self))]
    fn probes(&self) -> &[&dyn Probe] {
        self.probes
    }

    #[instrument(level = "trace", skip(self))]
    fn assessors(&self) -> &[&dyn Assessor] {
        self.assessors
    }

    #[instrument(level = "trace", skip(self))]
    fn workspace_assessors(&self) -> &[&dyn WorkspaceAssessor] {
        self.workspace_assessors.unwrap_or(&[])
    }

    #[instrument(level = "trace", skip(self))]
    fn reporters(&self) -> &[&dyn Reporter] {
        self.reporters
    }

    #[instrument(level = "trace", skip(self))]
    fn is_coverage(&self) -> bool {
        self.is_coverage
    }
}
