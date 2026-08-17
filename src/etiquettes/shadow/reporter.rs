use std::collections::BTreeMap;

use crate::error::CordialResult;
use crate::hooks::Reporter;
use crate::ir::IrView;
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};
use crate::session::SessionView;
use crate::shadow::api_family;

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

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
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
                    field(row, "item_path"),
                    field(row, "item_kind"),
                    api_family(field(row, "item_path")),
                    field(row, "status"),
                    field(row, "coverage_kind"),
                    field(row, "primary_gap_kind"),
                    field(row, "shadow_item"),
                    field(row, "drift_confidence"),
                    field(row, "shadow_elicit_impl"),
                    field(row, "verification_gap"),
                    field(row, "verification_ready"),
                    field(row, "shadow_can_be_direct"),
                    field(row, "shadow_missing_external_traits"),
                    field(row, "shadow_missing_our_traits"),
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

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
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
                field(row, "target_crate"),
                field(row, "shadow_crate"),
                field(row, "item_path"),
                field(row, "item_kind"),
                gap_kind,
                field(row, "shadow_item"),
                field(row, "drift_confidence"),
                field(row, "shadow_elicit_impl"),
                field(row, "shadow_can_be_direct"),
                field(row, "shadow_missing_external_traits"),
                field(row, "shadow_missing_our_traits"),
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

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
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

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
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
                field("crate"),
                field("target_path"),
                field("shadow_path"),
                finding.disposition()
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
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Artifact>>> {
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
