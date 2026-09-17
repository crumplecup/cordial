//! Quality-report area declarations and finding count helpers.

use crate::objects::{Disposition, Finding, MapFindingSink};

use tracing::instrument;

use super::traits::Etiquette;

/// This etiquette's own contribution to the workspace `quality-report.md`
/// rollup. Every quality etiquette must answer this through
/// [`crate::etiquette::StaticQualityEtiquette`]'s mandatory field.
pub trait QualityReportArea {
    /// This etiquette's row in the workspace quality-report rollup, if any.
    ///
    /// `None` must mean "intentionally absent", not "forgotten".
    fn quality_area(&self) -> Option<QualityAreaSpec>;
}

/// A quality (non-coverage) etiquette declares both its hook bundle and its
/// rollup contribution.
pub trait QualityEtiquette: Etiquette + QualityReportArea {}
impl<T: Etiquette + QualityReportArea + ?Sized> QualityEtiquette for T {}

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
    ///
    /// `compute` receives all findings and should count only the rows owned by
    /// this area.
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

/// Every finding in `findings` still open (not suppressed or an exemplar), the
/// standard scope for a quality-report area's own open-item count.
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
    sink.fields()
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}
