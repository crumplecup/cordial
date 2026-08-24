//! An **etiquette** is a named bundle of analysis hooks: loaders, enrichers,
//! probes, assessors, and reporters.
//!
//! Built-in etiquettes live under `src/etiquettes/`. Register one on a
//! [`crate::Session`] with [`crate::Session::register`], or run the CLI
//! (`cordial quality`, `cordial coverage`).

use crate::hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};
use crate::objects::{Disposition, Finding, MapFindingSink};

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

/// One resolution-priority row a quality etiquette contributes to the
/// workspace `quality-report.md` rollup.
#[derive(Debug, Clone, Copy)]
pub struct QualityAreaSpec {
    /// Display title for the resolution-order table ("Proof patterns").
    pub title: &'static str,
    /// Checklist artifact filename this etiquette's own reporter writes.
    pub checklist: &'static str,
    /// Summary artifact filename this etiquette's own reporter writes.
    pub summary: &'static str,
    /// Computes this area's own open-item count and one-line breakdown
    /// detail from the full session finding pool (not just this
    /// etiquette's own findings -- an area may need to look past its own
    /// category, the way none of the built-in areas currently do, but a
    /// third-party one might).
    pub compute: fn(&[&dyn Finding]) -> (usize, String),
}

/// This etiquette's own contribution to the workspace `quality-report.md`
/// rollup -- every quality (non-coverage) etiquette must answer this,
/// enforced by [`StaticQualityEtiquette`]'s mandatory field (no
/// `Default`, so a struct literal missing it is a compile error, not a
/// silent gap). `None` is a real, valid answer -- reference-only
/// inventory (no resolution strategy of its own, e.g. `error_sites`) or
/// findings folded into another etiquette's hand-composed area (e.g.
/// `panics` into "Error handling") both decline on purpose -- but every
/// etiquette must make that choice explicitly rather than defaulting to
/// invisible. See `docs/planning/quality-report-feeder-trait.md`.
pub trait QualityReportArea {
    fn quality_area(&self) -> Option<QualityAreaSpec>;
}

/// A quality (non-coverage) etiquette: declares its own hook bundle
/// (`Etiquette`) *and* its own rollup contribution (`QualityReportArea`).
/// Coverage etiquettes stay on plain [`StaticEtiquette`]/[`Etiquette`] --
/// they don't feed `quality-report.md` at all, a separate report entirely.
pub trait QualityEtiquette: Etiquette + QualityReportArea {}
impl<T: Etiquette + QualityReportArea + ?Sized> QualityEtiquette for T {}

/// Static quality-etiquette declaration: a [`StaticEtiquette`] plus its
/// mandatory rollup contribution. Composition, not field duplication --
/// `id`/`loaders`/etc. delegate straight through to the wrapped
/// `StaticEtiquette`.
pub struct StaticQualityEtiquette {
    pub etiquette: StaticEtiquette,
    pub quality_area: Option<QualityAreaSpec>,
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

/// Every finding in `findings` still open (not suppressed or an
/// exemplar) -- the standard scope for a quality-report area's own
/// open-item count.
#[instrument(level = "debug", skip(findings))]
pub(crate) fn open_findings<'a>(
    findings: &'a [&'a dyn Finding],
) -> impl Iterator<Item = &'a dyn Finding> + 'a {
    findings
        .iter()
        .copied()
        .filter(|finding| finding.disposition() == Disposition::Open)
}

/// Count of open findings in one rule category.
#[instrument(level = "debug", skip(findings))]
pub(crate) fn count_open_category(findings: &[&dyn Finding], category: &str) -> usize {
    open_findings(findings)
        .filter(|finding| finding.rule().category() == category)
        .count()
}

/// Count of open findings under one specific rule id.
#[instrument(level = "debug", skip(findings))]
pub(crate) fn count_open_rule(findings: &[&dyn Finding], rule_id: &str) -> usize {
    open_findings(findings)
        .filter(|finding| finding.rule().id() == rule_id)
        .count()
}

/// One emitted field's value off a finding, by name.
#[instrument(level = "debug", skip(finding))]
pub(crate) fn finding_field(finding: &dyn Finding, name: &str) -> Option<String> {
    let mut sink = MapFindingSink::default();
    finding.emit(&mut sink);
    sink.fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}
