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
    checklist_rules: &'static [&'static str],
    extra_checklists: &'static [QualityChecklistSpec],
    summary: &'static str,
    compute: fn(&[&dyn Finding]) -> (usize, String),
}

/// One checklist artifact and, optionally, the rule sections owned by an area.
#[derive(Debug, Clone, Copy)]
pub struct QualityChecklistSpec {
    name: &'static str,
    rules: &'static [&'static str],
}

impl QualityChecklistSpec {
    /// Read all open items from one checklist artifact.
    pub const fn new(name: &'static str) -> Self {
        Self { name, rules: &[] }
    }

    /// Read only items beneath these rule headings.
    pub const fn with_rules(mut self, rules: &'static [&'static str]) -> Self {
        self.rules = rules;
        self
    }

    /// Checklist artifact filename.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Included rule headings; empty means every open item.
    pub const fn rules(&self) -> &'static [&'static str] {
        self.rules
    }
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
            checklist_rules: &[],
            extra_checklists: &[],
            summary,
            compute,
        }
    }

    /// Add checklist artifacts that contribute to this area's worklist.
    pub const fn with_extra_checklists(
        mut self,
        checklists: &'static [QualityChecklistSpec],
    ) -> Self {
        self.extra_checklists = checklists;
        self
    }

    /// Limit the primary checklist to these rule sections.
    pub const fn with_checklist_rules(mut self, rules: &'static [&'static str]) -> Self {
        self.checklist_rules = rules;
        self
    }

    /// Display title for the resolution-order table ("Proof patterns").
    pub const fn title(&self) -> &'static str {
        self.title
    }

    /// Checklist artifact filename this etiquette's own reporter writes.
    pub const fn checklist(&self) -> &'static str {
        self.checklist
    }

    /// Additional checklist artifacts, in worklist order.
    pub const fn extra_checklists(&self) -> &'static [QualityChecklistSpec] {
        self.extra_checklists
    }

    /// Included rule headings in the primary checklist.
    pub const fn checklist_rules(&self) -> &'static [&'static str] {
        self.checklist_rules
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
pub fn open_findings<'a>(
    findings: &'a [&'a dyn Finding],
) -> impl Iterator<Item = &'a dyn Finding> + 'a {
    findings
        .iter()
        .copied()
        .filter(|finding| finding.disposition() == Disposition::Open)
}

/// Count of open findings in one rule category.
#[instrument(level = "debug", skip(findings))]
pub fn count_open_category(findings: &[&dyn Finding], category: &str) -> usize {
    open_findings(findings)
        .filter(|finding| finding.rule().category() == category)
        .count()
}

/// Count of open findings under one specific rule id.
#[instrument(level = "debug", skip(findings))]
pub fn count_open_rule(findings: &[&dyn Finding], rule_id: &str) -> usize {
    open_findings(findings)
        .filter(|finding| finding.rule().id() == rule_id)
        .count()
}

/// One emitted field's value off a finding, by name.
#[instrument(level = "debug", skip(finding))]
pub fn finding_field(finding: &dyn Finding, name: &str) -> Option<String> {
    let mut sink = MapFindingSink::default();
    finding.emit(&mut sink);
    sink.fields()
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}
