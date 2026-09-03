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
    id: &'static str,
    summary: &'static str,
}

impl EtiquetteRuleExplain {
    /// Bind a stable rule id to its one-line note.
    pub const fn new(id: &'static str, summary: &'static str) -> Self {
        Self { id, summary }
    }

    /// Stable rule identifier (`DOC-WARNING-001`).
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// One-line decision note for that rule.
    pub const fn summary(&self) -> &'static str {
        self.summary
    }
}

/// Why this etiquette exists and how to opt out.
///
/// Mandatory on [`StaticEtiquette`] (no [`Default`]): a constructor
/// call missing an argument is a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EtiquetteExplain {
    summary: &'static str,
    why: &'static str,
    logic: &'static str,
    opt_out: &'static str,
    rules: &'static [EtiquetteRuleExplain],
}

impl EtiquetteExplain {
    /// Bind the explain page for a static etiquette table.
    pub const fn new(
        summary: &'static str,
        why: &'static str,
        logic: &'static str,
        opt_out: &'static str,
        rules: &'static [EtiquetteRuleExplain],
    ) -> Self {
        Self {
            summary,
            why,
            logic,
            opt_out,
            rules,
        }
    }

    /// One line for `cordial explain` with no argument.
    pub const fn summary(&self) -> &'static str {
        self.summary
    }

    /// Why the check exists.
    pub const fn why(&self) -> &'static str {
        self.why
    }

    /// What is flagged, what is ignored, how the scan works.
    pub const fn logic(&self) -> &'static str {
        self.logic
    }

    /// `[panics] enabled = false` in cordial.toml — not rustc lint levels.
    pub const fn opt_out(&self) -> &'static str {
        self.opt_out
    }

    /// Rule ids that alias this page.
    pub const fn rules(&self) -> &'static [EtiquetteRuleExplain] {
        self.rules
    }
}

/// The hook slices an etiquette contributes, grouped so [`StaticEtiquette`]
/// binds them as one argument instead of six.
///
/// `const` statics cannot call `derive_builder::build`, so this is a
/// hand-written `const fn new`.
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
}

/// Static etiquette declaration backed by slices of trait object references.
///
/// Does not implement [`Default`]: `explain` (and the rest) must be
/// written out so a new bundle cannot ship without an explanation.
/// `derive_builder` is not `const`, so this table uses [`Self::new`].
pub struct StaticEtiquette {
    id: &'static str,
    name: &'static str,
    hooks: EtiquetteHooks,
    is_coverage: bool,
    explain: EtiquetteExplain,
}

impl StaticEtiquette {
    /// Bind a static hook table. Not a builder: `const` statics cannot
    /// call `derive_builder::build`.
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
        self.hooks.loaders
    }

    #[instrument(level = "trace", skip(self))]
    fn enrichers(&self) -> &[&dyn IrEnricher] {
        self.hooks.enrichers
    }

    #[instrument(level = "trace", skip(self))]
    fn probes(&self) -> &[&dyn Probe] {
        self.hooks.probes
    }

    #[instrument(level = "trace", skip(self))]
    fn assessors(&self) -> &[&dyn Assessor] {
        self.hooks.assessors
    }

    #[instrument(level = "trace", skip(self))]
    fn workspace_assessors(&self) -> &[&dyn WorkspaceAssessor] {
        self.hooks.workspace_assessors.unwrap_or(&[])
    }

    #[instrument(level = "trace", skip(self))]
    fn reporters(&self) -> &[&dyn Reporter] {
        self.hooks.reporters
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
    title: &'static str,
    checklist: &'static str,
    summary: &'static str,
    compute: fn(&[&dyn Finding]) -> (usize, String),
}

impl QualityAreaSpec {
    /// Bind a quality-report row for a static etiquette table.
    pub const fn new(
        title: &'static str,
        checklist: &'static str,
        summary: &'static str,
        compute: fn(&[&dyn Finding]) -> (usize, String),
    ) -> Self {
        Self {
            title,
            checklist,
            summary,
            compute,
        }
    }

    /// Display title for the resolution-order table ("Proof patterns").
    pub const fn title(&self) -> &'static str {
        self.title
    }

    /// Checklist artifact filename this etiquette's own reporter writes.
    pub const fn checklist(&self) -> &'static str {
        self.checklist
    }

    /// Summary artifact filename this etiquette's own reporter writes.
    pub const fn summary(&self) -> &'static str {
        self.summary
    }

    /// Computes this area's own open-item count and one-line breakdown.
    pub const fn compute(&self) -> fn(&[&dyn Finding]) -> (usize, String) {
        self.compute
    }
}

/// Static quality-etiquette declaration: a [`StaticEtiquette`] plus its
/// mandatory rollup contribution. Composition, not field duplication --
/// `id`/`loaders`/etc. delegate straight through to the wrapped
/// `StaticEtiquette`.
pub struct StaticQualityEtiquette {
    etiquette: StaticEtiquette,
    quality_area: Option<QualityAreaSpec>,
}

impl StaticQualityEtiquette {
    /// Wrap a hook table with its optional quality-report row.
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
                    .rules()
                    .iter()
                    .any(|rule| rule.id() == query)
            })
        })
}

/// One line per etiquette: id, then the one-line summary, sorted by id.
#[instrument(level = "debug", skip(etiquettes))]
pub fn render_explain_list(etiquettes: &[&dyn Etiquette]) -> String {
    let mut rows: Vec<(&str, &str)> = etiquettes
        .iter()
        .map(|etiquette| (etiquette.id(), etiquette.explain().summary()))
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
        explain.summary(),
        explain.why(),
        explain.logic(),
        explain.opt_out(),
    );
    if !explain.rules().is_empty() {
        body.push_str("\n## Rules\n\n");
        for rule in explain.rules() {
            let _ = writeln!(body, "- `{}` — {}", rule.id(), rule.summary());
        }
    }
    body
}
