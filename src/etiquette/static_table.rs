//! Static etiquette table implementations.

use crate::hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};

use tracing::instrument;

use super::explain::EtiquetteExplain;
use super::hooks::EtiquetteHooks;
use super::quality::{QualityAreaSpec, QualityReportArea};
use super::traits::Etiquette;

/// Static etiquette declaration backed by slices of trait object references.
///
/// Does not implement [`Default`]: `explain` (and the rest) must be written out
/// so a new bundle cannot ship without an explanation. `derive_builder` is not
/// `const`, so this table uses [`Self::new`].
pub struct StaticEtiquette {
    id: &'static str,
    name: &'static str,
    hooks: EtiquetteHooks,
    is_coverage: bool,
    explain: EtiquetteExplain,
}

impl StaticEtiquette {
    /// Bind a static hook table.
    ///
    /// Not a builder: `const` statics cannot call `derive_builder::build`.
    /// Keep `id`, rule ids, marker labels, and artifact names stable after
    /// users have generated reports or exceptions.
    pub const fn new(
        id: &'static str,
        name: &'static str,
        hooks: EtiquetteHooks,
        is_coverage: bool,
        explain: EtiquetteExplain,
    ) -> Self {
        Self {
            id,
            name,
            hooks,
            is_coverage,
            explain,
        }
    }
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
    fn explain(&self) -> EtiquetteExplain {
        self.explain
    }

    #[instrument(level = "trace", skip(self))]
    fn loaders(&self) -> &[&dyn Loader] {
        self.hooks.loaders()
    }

    #[instrument(level = "trace", skip(self))]
    fn enrichers(&self) -> &[&dyn IrEnricher] {
        self.hooks.enrichers()
    }

    #[instrument(level = "trace", skip(self))]
    fn probes(&self) -> &[&dyn Probe] {
        self.hooks.probes()
    }

    #[instrument(level = "trace", skip(self))]
    fn assessors(&self) -> &[&dyn Assessor] {
        self.hooks.assessors()
    }

    #[instrument(level = "trace", skip(self))]
    fn workspace_assessors(&self) -> &[&dyn WorkspaceAssessor] {
        self.hooks.workspace_assessors().unwrap_or(&[])
    }

    #[instrument(level = "trace", skip(self))]
    fn reporters(&self) -> &[&dyn Reporter] {
        self.hooks.reporters()
    }

    #[instrument(level = "trace", skip(self))]
    fn is_coverage(&self) -> bool {
        self.is_coverage
    }
}

/// Static quality-etiquette declaration: a [`StaticEtiquette`] plus its
/// mandatory rollup contribution. Composition, not field duplication:
/// `id`/`loaders`/etc. delegate straight through to the wrapped
/// `StaticEtiquette`.
pub struct StaticQualityEtiquette {
    etiquette: StaticEtiquette,
    quality_area: Option<QualityAreaSpec>,
}

impl StaticQualityEtiquette {
    /// Wrap a hook table with its optional quality-report row.
    ///
    /// Use `Some` for ordinary quality etiquettes. Use `None` only for explicit
    /// reference inventories or findings intentionally rolled into another
    /// hand-composed area.
    pub const fn new(etiquette: StaticEtiquette, quality_area: Option<QualityAreaSpec>) -> Self {
        Self {
            etiquette,
            quality_area,
        }
    }
}

impl Etiquette for StaticQualityEtiquette {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        self.etiquette.id()
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        self.etiquette.name()
    }

    #[instrument(level = "trace", skip(self))]
    fn explain(&self) -> EtiquetteExplain {
        self.etiquette.explain()
    }

    #[instrument(level = "trace", skip(self))]
    fn loaders(&self) -> &[&dyn Loader] {
        self.etiquette.loaders()
    }

    #[instrument(level = "trace", skip(self))]
    fn enrichers(&self) -> &[&dyn IrEnricher] {
        self.etiquette.enrichers()
    }

    #[instrument(level = "trace", skip(self))]
    fn probes(&self) -> &[&dyn Probe] {
        self.etiquette.probes()
    }

    #[instrument(level = "trace", skip(self))]
    fn assessors(&self) -> &[&dyn Assessor] {
        self.etiquette.assessors()
    }

    #[instrument(level = "trace", skip(self))]
    fn workspace_assessors(&self) -> &[&dyn WorkspaceAssessor] {
        self.etiquette.workspace_assessors()
    }

    #[instrument(level = "trace", skip(self))]
    fn reporters(&self) -> &[&dyn Reporter] {
        self.etiquette.reporters()
    }

    #[instrument(level = "trace", skip(self))]
    fn is_coverage(&self) -> bool {
        self.etiquette.is_coverage()
    }
}

impl QualityReportArea for StaticQualityEtiquette {
    #[instrument(level = "trace", skip(self))]
    fn quality_area(&self) -> Option<QualityAreaSpec> {
        self.quality_area
    }
}
