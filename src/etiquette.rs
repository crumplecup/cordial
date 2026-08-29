//! An **etiquette** is a named bundle of analysis hooks: loaders, enrichers,
//! probes, assessors, and reporters.
//!
//! Built-in etiquettes live under `src/etiquettes/`. Register one on a
//! [`crate::Session`] with [`crate::Session::register`], or run the CLI
//! (`cordial quality`, `cordial coverage`). `cordial explain` prints
//! [`Etiquette::explain`] for every bundle compiled into the binary.

use std::fmt::Write;

use crate::hooks::{Assessor, IrEnricher, Loader, Probe, Reporter, WorkspaceAssessor};
use crate::objects::{Disposition, Finding, MapFindingSink};

use tracing::instrument;
/// Named bundle of cordial hook implementations.
pub trait Etiquette: Send + Sync {
    /// Stable identifier for this hook.
    fn id(&self) -> &str;
    /// Human-readable name.
    fn name(&self) -> &str;

    /// Why this check exists, what it flags, and how to opt out.
    ///
    /// Required: a new etiquette that forgets this is a compile error, not
    /// a silent gap. See `docs/planning/etiquette-explain.md`.
    fn explain(&self) -> EtiquetteExplain;

    /// Loaders that populate IR for this etiquette.
    fn loaders(&self) -> &[&dyn Loader];
    /// Enrichers that run after loaders.
    fn enrichers(&self) -> &[&dyn IrEnricher];
    /// Probes that attach markers to the IR.
    fn probes(&self) -> &[&dyn Probe];
    /// Assessors that turn markers into findings.
    fn assessors(&self) -> &[&dyn Assessor];
    /// Optional workspace-scoped assessors; empty by default.
    fn workspace_assessors(&self) -> &[&dyn WorkspaceAssessor] {
        &[]
    }
    /// Reporters that render findings into artifacts.
    fn reporters(&self) -> &[&dyn Reporter];

    /// True for trait-impl / framework coverage hook bundles (not source-quality scans).
    fn is_coverage(&self) -> bool {
        false
    }
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
    /// This etiquette's row in the workspace quality-report rollup, if any.
    fn quality_area(&self) -> Option<QualityAreaSpec>;
}

/// A quality (non-coverage) etiquette: declares its own hook bundle
/// (`Etiquette`) *and* its own rollup contribution (`QualityReportArea`).
/// Coverage etiquettes stay on plain [`StaticEtiquette`]/[`Etiquette`] --
/// they don't feed `quality-report.md` at all, a separate report entirely.
pub trait QualityEtiquette: Etiquette + QualityReportArea {}
impl<T: Etiquette + QualityReportArea + ?Sized> QualityEtiquette for T {}

/// One rule id this etiquette can emit, so `cordial explain RULE-ID`
/// resolves to the etiquette page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EtiquetteRuleExplain {
    /// Stable rule identifier (`DOC-WARNING-001`).
    pub id: &'static str,
    /// One-line decision note for that rule.
    pub summary: &'static str,
}

/// Why this etiquette exists and how to opt out.
///
/// Mandatory on [`StaticEtiquette`] (no [`Default`]): a struct literal
/// missing the field is a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EtiquetteExplain {
    /// One line for `cordial explain` with no argument.
    pub summary: &'static str,
    /// Why the check exists.
    pub why: &'static str,
    /// What is flagged, what is ignored, how the scan works.
    pub logic: &'static str,
    /// `[panics] enabled = false` in cordial.toml — not rustc lint levels.
    pub opt_out: &'static str,
    /// Rule ids that alias this page.
    pub rules: &'static [EtiquetteRuleExplain],
}

/// Static etiquette declaration backed by slices of trait object references.
///
/// Does not implement [`Default`]: `explain` (and the rest) must be
/// written out so a new bundle cannot ship without an explanation.
pub struct StaticEtiquette {
    /// Stable identifier.
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Loaders in this etiquette.
    pub loaders: &'static [&'static dyn Loader],
    /// Enrichers in this etiquette.
    pub enrichers: &'static [&'static dyn IrEnricher],
    /// Probes in this etiquette.
    pub probes: &'static [&'static dyn Probe],
    /// Assessors in this etiquette.
    pub assessors: &'static [&'static dyn Assessor],
    /// Workspace-scoped assessors, if any.
    pub workspace_assessors: Option<&'static [&'static dyn WorkspaceAssessor]>,
    /// Reporters in this etiquette.
    pub reporters: &'static [&'static dyn Reporter],
    /// Whether this bundle is a coverage etiquette.
    pub is_coverage: bool,
    /// Why this check exists and how to opt out.
    pub explain: EtiquetteExplain,
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

/// Static quality-etiquette declaration: a [`StaticEtiquette`] plus its
/// mandatory rollup contribution. Composition, not field duplication --
/// `id`/`loaders`/etc. delegate straight through to the wrapped
/// `StaticEtiquette`.
pub struct StaticQualityEtiquette {
    /// Hook bundle this quality etiquette wraps.
    pub etiquette: StaticEtiquette,
    /// Optional quality-report rollup contribution.
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

/// First etiquette whose id or rule id equals `query`.
#[instrument(level = "debug", skip(etiquettes))]
pub fn lookup_etiquette<'a>(
    etiquettes: &[&'a dyn Etiquette],
    query: &str,
) -> Option<&'a dyn Etiquette> {
    etiquettes
        .iter()
        .copied()
        .find(|etiquette| etiquette.id() == query)
        .or_else(|| {
            etiquettes.iter().copied().find(|etiquette| {
                etiquette
                    .explain()
                    .rules
                    .iter()
                    .any(|rule| rule.id == query)
            })
        })
}

/// One line per etiquette: id, then the one-line summary, sorted by id.
#[instrument(level = "debug", skip(etiquettes))]
pub fn render_explain_list(etiquettes: &[&dyn Etiquette]) -> String {
    let mut rows: Vec<(&str, &str)> = etiquettes
        .iter()
        .map(|etiquette| (etiquette.id(), etiquette.explain().summary))
        .collect();
    rows.sort_by(|left, right| left.0.cmp(right.0));
    let width = rows.iter().map(|(id, _)| id.len()).max().unwrap_or(0);
    let mut body = String::new();
    for (id, summary) in rows {
        let _ = writeln!(body, "{id:<width$}  {summary}");
    }
    body
}

/// Full explain page for one etiquette.
#[instrument(level = "debug", skip(etiquette))]
pub fn render_explain_page(etiquette: &dyn Etiquette) -> String {
    let explain = etiquette.explain();
    let mut body = format!(
        "# {} (`{}`)\n\n{}\n\n## Why\n\n{}\n\n## Logic\n\n{}\n\n## Opt out\n\n{}\n",
        etiquette.name(),
        etiquette.id(),
        explain.summary,
        explain.why,
        explain.logic,
        explain.opt_out,
    );
    if !explain.rules.is_empty() {
        body.push_str("\n## Rules\n\n");
        for rule in explain.rules {
            let _ = writeln!(body, "- `{}` — {}", rule.id, rule.summary);
        }
    }
    body
}
