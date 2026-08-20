use crate::objects::{Finding, MapFindingSink};

use super::super::hierarchy::ModuleSizeInput;

use tracing::instrument;
pub(super) const SUMMARY_MODULE_ROWS: usize = 20;
pub(super) const SUMMARY_RANK_ROWS: usize = 10;
pub(super) const HOTSPOT_METHODS: usize = 3;

#[derive(Debug, Default, Clone)]
pub(super) struct ModularityRow {
    pub crate_name: String,
    pub kind: String,
    pub context: String,
    pub file: String,
    pub line: String,
    pub lines: String,
    pub checklist: String,
    pub disposition: String,
    pub zscore: String,
    pub inline: String,
    pub share: String,
    pub detail: String,
}

impl ModularityRow {
    #[instrument(level = "debug", skip(finding), ret)]
    pub(super) fn from_finding(finding: &dyn Finding) -> Self {
        let mut sink = MapFindingSink::default();
        finding.emit(&mut sink);
        let field = |name: &str| {
            sink.fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        Self {
            crate_name: field("crate"),
            kind: field("kind"),
            context: field("context"),
            file: field("file"),
            line: field("line"),
            lines: field("lines"),
            checklist: field("checklist"),
            disposition: finding.disposition().to_string(),
            zscore: field("zscore"),
            inline: field("inline"),
            share: field("share"),
            detail: field("detail"),
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn line_count(&self) -> u32 {
        self.lines.parse().unwrap_or(0)
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn is_checklist(&self) -> bool {
        self.checklist == "true"
    }
}

#[instrument(level = "trace", skip(row, thresholds))]
pub(super) fn is_inventory_row(
    row: &ModularityRow,
    thresholds: &crate::config::ModularityThresholds,
) -> bool {
    if row.kind == "MODULARITY-FUNCTION" {
        row.line_count() >= thresholds.function_inventory_min_lines()
    } else {
        true
    }
}

#[instrument(level = "debug", skip(findings))]
pub(super) fn modularity_rows(findings: &[&dyn Finding]) -> Vec<ModularityRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "modularity")
        .map(|finding| ModularityRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
pub(super) fn open_rows(rows: &[ModularityRow]) -> impl Iterator<Item = &ModularityRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

#[instrument(level = "debug", skip(rows))]
pub(super) fn crate_names(rows: &[&ModularityRow]) -> Vec<String> {
    let mut names: Vec<String> = rows.iter().map(|row| row.crate_name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

#[instrument(level = "debug", skip(rows))]
pub(super) fn sort_by_lines_desc(rows: &mut [&ModularityRow]) {
    rows.sort_by(|left, right| {
        right
            .line_count()
            .cmp(&left.line_count())
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.context.cmp(&right.context))
    });
}

#[instrument(level = "debug", skip(rows))]
pub(super) fn count_kind(rows: &[&ModularityRow], kind: &str) -> usize {
    rows.iter().filter(|row| row.kind == kind).count()
}

#[instrument(level = "debug", skip(rows))]
pub(super) fn max_lines(rows: &[&ModularityRow], kind: &str) -> u32 {
    rows.iter()
        .filter(|row| row.kind == kind)
        .map(|row| row.line_count())
        .max()
        .unwrap_or(0)
}

#[instrument(level = "debug", skip(rows))]
pub(super) fn file_module_inputs(rows: &[&ModularityRow]) -> Vec<ModuleSizeInput> {
    rows.iter()
        .filter(|row| row.kind == "MODULARITY-MODULE-SIZE" && row.inline != "true")
        .filter_map(|row| {
            Some(ModuleSizeInput {
                path: row.context.clone(),
                file: row.file.clone(),
                lines: row.lines.parse().ok()?,
            })
        })
        .collect()
}
