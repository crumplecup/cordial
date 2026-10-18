//! Adds finding identity to the checklist's human context.

use crate::objects::{Disposition, Finding, MapFindingSink};
use crate::{CordialResult, QualityAreaSummary};

use super::{ChecklistSource, ItemSource, WorklistItem};

use tracing::instrument;

/// A checklist source enriched with the structured findings from the same gate.
pub struct FindingSource {
    checklists: ChecklistSource,
    findings: Vec<FindingIdentity>,
}

#[derive(Debug)]
struct FindingIdentity {
    category: String,
    rule_id: String,
    crate_name: Option<String>,
    file: Option<String>,
    line: Option<u32>,
    context: Option<String>,
}

struct MatchedFinding {
    rule_id: String,
    crate_name: Option<String>,
    file: Option<String>,
    line: Option<u32>,
}

impl FindingSource {
    /// Read the gate's finished checklists and its open finding identities.
    #[instrument(level = "debug", skip(checklists, findings))]
    pub fn from_findings(checklists: ChecklistSource, findings: &[&dyn Finding]) -> Self {
        let findings = findings
            .iter()
            .filter(|finding| finding.disposition() == Disposition::Open)
            .map(|finding| {
                let mut sink = MapFindingSink::default();
                finding.emit(&mut sink);
                let field = |name: &str| {
                    sink.fields()
                        .iter()
                        .find(|(key, _)| key == name)
                        .map(|(_, value)| value.clone())
                        .filter(|value| !value.is_empty())
                };
                FindingIdentity {
                    category: finding.rule().category().to_string(),
                    rule_id: finding.rule().id().to_string(),
                    crate_name: field("crate"),
                    file: field("file"),
                    line: field("line").and_then(|value| value.parse().ok()),
                    context: field("context")
                        .or_else(|| field("dependency"))
                        .or_else(|| field("qualified_name")),
                }
            })
            .collect();
        Self {
            checklists,
            findings,
        }
    }
}

impl ItemSource for FindingSource {
    #[instrument(level = "trace", skip(self, area))]
    fn items(&self, area: &QualityAreaSummary) -> CordialResult<Vec<WorklistItem>> {
        let mut items = self.checklists.items(area)?;
        for item in &mut items {
            if let Some(finding) = self.match_item(item) {
                item.set_finding_source(
                    &finding.rule_id,
                    finding.crate_name.as_deref(),
                    finding.file.as_deref(),
                    finding.line,
                );
            }
        }
        Ok(items)
    }
}

impl FindingSource {
    #[instrument(level = "trace", skip(self, item))]
    fn match_item(&self, item: &WorklistItem) -> Option<MatchedFinding> {
        let category = checklist_category(item.checklist());
        let location_line = location_line(item.item_line(), item.file().as_deref());
        let visible =
            item.heading_trail()
                .iter()
                .fold(item.item_line().clone(), |mut text, heading| {
                    text.push_str(heading);
                    text
                });
        let mut candidates: Vec<_> = self
            .findings
            .iter()
            .filter(|finding| finding.category == category)
            .filter(|finding| {
                item.file().as_ref().is_none_or(|file| {
                    finding
                        .file
                        .as_deref()
                        .is_some_and(|source| same_file(file, source))
                })
            })
            .filter(|finding| location_line.is_none_or(|line| finding.line == Some(line)))
            .collect();
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() > 1 {
            let by_rule: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|finding| visible.contains(&finding.rule_id))
                .collect();
            if !by_rule.is_empty() {
                candidates = by_rule;
            }
        }
        if candidates.len() > 1 {
            let by_context: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|finding| {
                    finding
                        .context
                        .as_deref()
                        .is_some_and(|context| visible.contains(context))
                })
                .collect();
            if !by_context.is_empty() {
                candidates = by_context;
            }
        }
        let first = *candidates.first()?;
        if !candidates
            .iter()
            .all(|finding| finding.rule_id == first.rule_id)
        {
            return None;
        }
        let shared = |extract: fn(&FindingIdentity) -> Option<String>| {
            let value = extract(first);
            candidates
                .iter()
                .all(|finding| extract(finding) == value)
                .then_some(value)
                .flatten()
        };
        let line = candidates
            .iter()
            .all(|finding| finding.line == first.line)
            .then_some(first.line)
            .flatten();
        Some(MatchedFinding {
            rule_id: first.rule_id.clone(),
            crate_name: shared(|finding| finding.crate_name.clone()),
            file: shared(|finding| finding.file.clone()),
            line,
        })
    }
}

#[instrument(level = "trace")]
fn checklist_category(checklist: &str) -> String {
    let stem = checklist.strip_suffix(".checklist.md").unwrap_or(checklist);
    if stem.starts_with("tracing-") {
        return "tracing".to_string();
    }
    if stem == "version-in-member" {
        return "antipatterns".to_string();
    }
    stem.replace('-', "_")
}

#[instrument(level = "trace")]
fn same_file(left: &str, right: &str) -> bool {
    left.trim_start_matches("./") == right.trim_start_matches("./")
}

#[instrument(level = "debug", skip(file))]
fn location_line(item_line: &str, file: Option<&str>) -> Option<u32> {
    let file = file?;
    item_line
        .split('`')
        .filter_map(|quoted| quoted.rsplit_once(':'))
        .find_map(|(path, line)| same_file(path, file).then(|| line.parse().ok()).flatten())
}
