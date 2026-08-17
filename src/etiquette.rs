use crate::hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};

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
    fn id(&self) -> &str {
        self.id
    }

    fn name(&self) -> &str {
        self.name
    }

    fn loaders(&self) -> &[&dyn Loader] {
        self.loaders
    }

    fn enrichers(&self) -> &[&dyn IrEnricher] {
        self.enrichers
    }

    fn probes(&self) -> &[&dyn Probe] {
        self.probes
    }

    fn assessors(&self) -> &[&dyn Assessor] {
        self.assessors
    }

    fn workspace_assessors(&self) -> &[&dyn WorkspaceAssessor] {
        self.workspace_assessors.unwrap_or(&[])
    }

    fn reporters(&self) -> &[&dyn Reporter] {
        self.reporters
    }

    fn is_coverage(&self) -> bool {
        self.is_coverage
    }
}
