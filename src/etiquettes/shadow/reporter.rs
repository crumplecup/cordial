use std::collections::BTreeMap;

use crate::csv_row::csv_field as csv_escape;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};
use crate::shadow::api_family;

use tracing::instrument;
#[instrument(level = "debug", skip(findings))]
fn pair_rows(findings: &[&dyn Finding]) -> Vec<MapFindingSink> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "shadow-pair")
        .map(|finding| {
            let mut sink = MapFindingSink::default();
            finding.emit(&mut sink);
            sink
        })
        .collect()
}

#[instrument(level = "debug", skip(sink))]
fn field<'a>(sink: &'a MapFindingSink, name: &str) -> &'a str {
    sink.fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ShadowPairCsvReporter;

impl ShadowPairCsvReporter {
    pub const ID: &'static str = "shadow-pair-csv";
}

impl Reporter for ShadowPairCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = pair_rows(findings);
        let mut by_target: BTreeMap<String, Vec<&MapFindingSink>> = BTreeMap::new();
        for row in &rows {
            by_target
                .entry(field(row, "target_crate").to_string())
                .or_default()
                .push(row);
        }

        let mut artifacts = Vec::new();
        for (target_crate, target_rows) in by_target {
            let mut body = String::from(
                "item_path,item_kind,api_family,status,coverage_kind,primary_gap_kind,shadow_item,drift_confidence,shadow_elicit_impl,verification_gap,verification_ready,shadow_can_be_direct,shadow_missing_external_traits,shadow_missing_our_traits,action,notes\n",
            );
            let mut sorted = target_rows;
            sorted.sort_by(|left, right| field(left, "item_path").cmp(field(right, "item_path")));
            for row in sorted {
                body.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                    csv_escape(field(row, "item_path")),
                    csv_escape(field(row, "item_kind")),
                    csv_escape(&api_family(field(row, "item_path"))),
                    csv_escape(field(row, "status")),
                    csv_escape(field(row, "coverage_kind")),
                    csv_escape(field(row, "primary_gap_kind")),
                    csv_escape(field(row, "shadow_item")),
                    csv_escape(field(row, "drift_confidence")),
                    csv_escape(field(row, "shadow_elicit_impl")),
                    csv_escape(field(row, "verification_gap")),
                    csv_escape(field(row, "verification_ready")),
                    csv_escape(field(row, "shadow_can_be_direct")),
                    csv_escape(field(row, "shadow_missing_external_traits")),
                    csv_escape(field(row, "shadow_missing_our_traits")),
                    csv_escape(field(row, "action")),
                    csv_escape(field(row, "notes")),
                ));
            }
            artifacts.push(Box::new(TextArtifact {
                name: format!("shadow-{target_crate}.csv"),
                media_type: "text/csv".to_string(),
                body,
            }) as Box<dyn Artifact>);
        }
        Ok(artifacts)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ShadowGapsCsvReporter;

impl ShadowGapsCsvReporter {
    pub const ID: &'static str = "shadow-gaps-csv";
}

impl Reporter for ShadowGapsCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from(
            "target_crate,shadow_crate,item_path,item_kind,gap_kind,matched_shadow_item,drift_confidence,shadow_elicit_impl,shadow_can_be_direct,shadow_missing_external_traits,shadow_missing_our_traits,action,notes\n",
        );
        let pair_rows = pair_rows(findings);
        let mut rows: Vec<_> = pair_rows
            .iter()
            .filter(|row| {
                !field(row, "primary_gap_kind").is_empty()
                    || field(row, "verification_gap") == "true"
            })
            .collect();
        rows.sort_by(|left, right| {
            field(left, "item_path")
                .cmp(field(right, "item_path"))
                .then(field(left, "target_crate").cmp(field(right, "target_crate")))
        });
        for row in rows {
            let gap_kind = if field(row, "verification_gap") == "true"
                && field(row, "primary_gap_kind").is_empty()
            {
                "ShadowVerificationGap"
            } else {
                field(row, "primary_gap_kind")
            };
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                csv_escape(field(row, "target_crate")),
                csv_escape(field(row, "shadow_crate")),
                csv_escape(field(row, "item_path")),
                csv_escape(field(row, "item_kind")),
                csv_escape(gap_kind),
                csv_escape(field(row, "shadow_item")),
                csv_escape(field(row, "drift_confidence")),
                csv_escape(field(row, "shadow_elicit_impl")),
                csv_escape(field(row, "shadow_can_be_direct")),
                csv_escape(field(row, "shadow_missing_external_traits")),
                csv_escape(field(row, "shadow_missing_our_traits")),
                csv_escape(field(row, "action")),
                csv_escape(field(row, "notes")),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "gaps-shadow.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ShadowCsvReporter;

impl ShadowCsvReporter {
    pub const ID: &'static str = "shadow-csv";
}

impl Reporter for ShadowCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from("crate,target_path,shadow_path,disposition\n");
        for finding in findings
            .iter()
            .filter(|finding| finding.rule().category() == "shadow")
        {
            let mut sink = MapFindingSink::default();
            finding.emit(&mut sink);
            let field = |name: &str| {
                sink.fields
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.as_str())
                    .unwrap_or("")
            };
            body.push_str(&format!(
                "{},{},{},{}\n",
                csv_escape(field("crate")),
                csv_escape(field("target_path")),
                csv_escape(field("shadow_path")),
                csv_escape(&finding.disposition().to_string())
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "shadow.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ShadowMethodChecklistReporter;

impl ShadowMethodChecklistReporter {
    pub const ID: &'static str = "shadow-method-checklist";
}

impl Reporter for ShadowMethodChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut artifacts = Vec::new();
        for finding in findings
            .iter()
            .filter(|finding| finding.rule().category() == "shadow-pair-checklist")
        {
            let mut sink = MapFindingSink::default();
            finding.emit(&mut sink);
            let field = |name: &str| {
                sink.fields
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.as_str())
                    .unwrap_or("")
            };
            let target_crate = field("target_crate");
            if target_crate.is_empty() {
                continue;
            }
            artifacts.push(Box::new(TextArtifact {
                name: format!("shadow-{target_crate}.checklist.md"),
                media_type: "text/markdown".to_string(),
                body: field("body").to_string(),
            }) as Box<dyn Artifact>);
        }
        Ok(artifacts)
    }
}
