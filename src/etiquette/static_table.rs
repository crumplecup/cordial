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
/// so a new bundle cannot ship without an explanation.
///
/// Deliberately a plain constructor, not `derive_builder`, despite 5 fields:
/// every field is always required, with no real validation, so a
/// builder's `.build()` would need `.expect()`/`.unwrap()` at every call
/// site to resolve its `Result` — all ~30 of which build inside
/// `LazyLock::new(|| ...)` closures that must return the value directly
/// and structurally cannot propagate an error. That trades a type-enforced
/// guarantee (every field supplied, checked by the compiler) for a
/// runtime-checked one (same guarantee, but panics in library code if
/// ever violated) for zero actual benefit — confirmed by `cordial`'s own
/// panics etiquette, which flagged the resulting 60+ new `.expect()` abort
/// sites once this was tried.
///
/// `id`/`name` are owned `String`, not `&'static str`: built-ins bind
/// literals (`String: From<&str>`) and etiquettes derived from
/// `cordial.toml` at runtime bind owned strings through the exact same
/// constructor. `id` is fixed once built either way, which is what
/// actually keeps exceptions and reports matching across runs — that
/// guarantee never came from `const`, just from nothing mutating these
/// fields after construction. Built-ins register through a `LazyLock`
/// (see e.g. `etiquettes::panics::PANICS_ETIQUETTE`) instead of a bare
/// `static`, since building one is no longer a `const` operation.
pub struct StaticEtiquette {
    id: String,
    name: String,
    hooks: EtiquetteHooks,
    is_coverage: bool,
    explain: EtiquetteExplain,
}

impl StaticEtiquette {
    /// Bind a hook table to its id, name, and explain page.
    ///
    /// Keep `id`, rule ids, marker labels, and artifact names stable after
    /// users have generated reports or exceptions.
    #[instrument(level = "debug", skip(id, name, hooks, explain))]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        hooks: EtiquetteHooks,
        is_coverage: bool,
        explain: EtiquetteExplain,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            hooks,
            is_coverage,
            explain,
        }
    }
}

impl Etiquette for StaticEtiquette {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        &self.id
    }

    #[instrument(level = "trace", skip(self))]
    fn name(&self) -> &str {
        &self.name
    }

    #[instrument(level = "trace", skip(self))]
    fn explain(&self) -> EtiquetteExplain {
        self.explain.clone()
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
