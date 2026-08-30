use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::types::{
    ForeignErrorAttenuationRecord, ForeignErrorAttenuationReport, ForeignErrorHandlingClass,
    WorkspaceForeignErrorAttenuationSummary, build_workspace_foreign_error_attenuation_summary,
};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct ForeignErrorAttenuationRow {
    crate_name: String,
    handling_class: String,
    resolution_id: String,
    foreign_error_type: String,
    inference_rule_id: String,
    confidence: String,
    context: String,
    file: String,
    line: String,
    site_kind: String,
    source_snippet: String,
    site_snippet: String,
    resolution: String,
    good_pattern: String,
    bad_pattern: String,
    disposition: String,
}

impl ForeignErrorAttenuationRow {
    #[instrument(level = "debug", skip(finding), ret)]
    fn from_finding(finding: &dyn Finding) -> Self {
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
            handling_class: field("handling_class"),
            resolution_id: field("resolution_id"),
            foreign_error_type: field("foreign_error_type"),
            inference_rule_id: field("inference_rule_id"),
            confidence: field("confidence"),
            context: field("context"),
            file: field("file"),
            line: field("line"),
            site_kind: field("site_kind"),
            source_snippet: field("source_snippet"),
            site_snippet: field("site_snippet"),
            resolution: field("resolution"),
            good_pattern: field("good_pattern"),
            bad_pattern: field("bad_pattern"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn attenuation_rows(findings: &[&dyn Finding]) -> Vec<ForeignErrorAttenuationRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "foreign_error_attenuation")
        .map(|finding| ForeignErrorAttenuationRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(
    rows: &[ForeignErrorAttenuationRow],
) -> impl Iterator<Item = &ForeignErrorAttenuationRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

#[instrument(level = "debug", skip(rows))]
fn report_from_rows(rows: &[ForeignErrorAttenuationRow]) -> ForeignErrorAttenuationReport {
    let crate_name = rows
        .first()
        .map(|row| row.crate_name.clone())
        .unwrap_or_default();
    let findings = rows
        .iter()
        .map(|row| ForeignErrorAttenuationRecord {
            crate_name: row.crate_name.clone(),
            foreign_error_type: row.foreign_error_type.clone(),
            inference_rule_id: row.inference_rule_id.clone(),
            confidence: if row.confidence.contains("MEDIUM") {
                crate::etiquettes::error_sites::ForeignTypeConfidence::Medium
            } else {
                crate::etiquettes::error_sites::ForeignTypeConfidence::High
            },
            handling_class: parse_handling_class(&row.handling_class),
            resolution_id: parse_resolution_id(&row.resolution_id),
            resolution: row.resolution.clone(),
            kind: parse_site_kind(&row.site_kind),
            context: row.context.clone(),
            file: std::path::PathBuf::from(&row.file),
            line: row.line.parse().unwrap_or(0),
            source_snippet: row.source_snippet.clone(),
            site_snippet: row.site_snippet.clone(),
            good_pattern: row.good_pattern.clone(),
            bad_pattern: row.bad_pattern.clone(),
        })
        .collect();
    ForeignErrorAttenuationReport {
        crate_name,
        findings,
    }
}

#[instrument(level = "debug")]
fn parse_handling_class(value: &str) -> ForeignErrorHandlingClass {
    if value.contains("CHAIN-PRESERVED") {
        ForeignErrorHandlingClass::ChainPreserved
    } else if value.contains("CHAIN-BREAK") {
        ForeignErrorHandlingClass::ChainBreak
    } else if value.contains("PENDING-INFRA") {
        ForeignErrorHandlingClass::PendingInfrastructure
    } else {
        ForeignErrorHandlingClass::Neutral
    }
}

#[instrument(level = "debug")]
fn parse_resolution_id(value: &str) -> super::types::ErrorHandlingResolutionId {
    use super::types::ErrorHandlingResolutionId;
    if value.contains("MAINTAIN-EXEMPLAR") {
        ErrorHandlingResolutionId::MaintainExemplar
    } else if value.contains("REPLACE-STRINGIFY-MAP-ERR") {
        ErrorHandlingResolutionId::ReplaceStringifyingMapErr
    } else if value.contains("ADD-INFRA-THEN-QUESTION-MARK") {
        ErrorHandlingResolutionId::AddInfrastructureThenQuestionMark
    } else {
        ErrorHandlingResolutionId::ManualReview
    }
}

#[instrument(level = "debug")]
fn parse_site_kind(value: &str) -> crate::etiquettes::error_sites::ErrorSiteKind {
    use crate::etiquettes::error_sites::ErrorSiteKind;
    match value {
        s if s == ErrorSiteKind::QuestionMark.to_string() => ErrorSiteKind::QuestionMark,
        s if s == ErrorSiteKind::MapErr.to_string() => ErrorSiteKind::MapErr,
        s if s == ErrorSiteKind::ReturnErr.to_string() => ErrorSiteKind::ReturnErr,
        s if s == ErrorSiteKind::IfLetErr.to_string() => ErrorSiteKind::IfLetErr,
        s if s == ErrorSiteKind::MatchErr.to_string() => ErrorSiteKind::MatchErr,
        s if s == ErrorSiteKind::OkOr.to_string() => ErrorSiteKind::OkOr,
        _ => ErrorSiteKind::QuestionMark,
    }
}

/// Writes `foreign-error-attenuation.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorAttenuationCsvReporter;

impl ForeignErrorAttenuationCsvReporter {
    pub const ID: &'static str = "foreign-error-attenuation-csv";
}

impl Reporter for ForeignErrorAttenuationCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from(
            "crate,handling_class,resolution_id,foreign_error_type,inference_rule_id,confidence,context,file,line,site_kind,source_snippet,site_snippet,resolution,good_pattern,bad_pattern\n",
        );
        for row in open_rows(&attenuation_rows(findings)) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.handling_class),
                csv_field(&row.resolution_id),
                csv_field(&row.foreign_error_type),
                csv_field(&row.inference_rule_id),
                csv_field(&row.confidence),
                csv_field(&row.context),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.site_kind),
                csv_field(&row.source_snippet),
                csv_field(&row.site_snippet),
                csv_field(&row.resolution),
                csv_field(&row.good_pattern),
                csv_field(&row.bad_pattern),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "foreign-error-attenuation.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `foreign-error-attenuation.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorAttenuationChecklistReporter;

impl ForeignErrorAttenuationChecklistReporter {
    pub const ID: &'static str = "foreign-error-attenuation-checklist";
}

impl Reporter for ForeignErrorAttenuationChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = attenuation_rows(findings);
        let report = report_from_rows(&rows);

        let mut body = String::new();
        body.push_str("# Foreign error handling attenuation\n\n");
        body.push_str(
            "Positive probe (`error-chain-preserved.*`) vs negative probe \
             (`foreign-error-types.*` chain breaks). Each row pairs **bad code** at the site with \
             **good code** and a baked-in **resolution** strategy.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        write_class_section(
            &mut body,
            &report,
            ForeignErrorHandlingClass::ChainPreserved,
            true,
        );
        write_class_section(
            &mut body,
            &report,
            ForeignErrorHandlingClass::ChainBreak,
            false,
        );
        write_class_section(
            &mut body,
            &report,
            ForeignErrorHandlingClass::PendingInfrastructure,
            false,
        );
        write_class_section(
            &mut body,
            &report,
            ForeignErrorHandlingClass::Neutral,
            false,
        );

        Ok(vec![Box::new(TextArtifact {
            name: "foreign-error-attenuation.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

#[instrument(level = "info", skip(report, class))]
fn write_class_section(
    body: &mut String,
    report: &ForeignErrorAttenuationReport,
    class: ForeignErrorHandlingClass,
    checked: bool,
) {
    let rows: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.handling_class == class)
        .collect();
    if rows.is_empty() {
        return;
    }

    let title = match class {
        ForeignErrorHandlingClass::ChainPreserved => "Chain preserved (good code)",
        ForeignErrorHandlingClass::ChainBreak => "Chain break (bad code → migrate)",
        ForeignErrorHandlingClass::PendingInfrastructure => {
            "Pending infrastructure (good code after `From` wiring)"
        }
        ForeignErrorHandlingClass::Neutral => "Neutral (manual review)",
    };
    body.push_str(&format!("### {title}\n\n"));

    let mut by_type: BTreeMap<&str, Vec<&ForeignErrorAttenuationRecord>> = BTreeMap::new();
    for row in rows {
        by_type
            .entry(row.foreign_error_type.as_str())
            .or_default()
            .push(row);
    }

    for (foreign_type, type_rows) in by_type {
        body.push_str(&format!("#### `{foreign_type}`\n\n"));
        for row in type_rows {
            let mark = if checked { 'x' } else { ' ' };
            body.push_str(&format!(
                "- [{mark}] `{context}` — `{file}:{line}` — `{resolution_id}`\n",
                context = row.context,
                file = row.file.display(),
                line = row.line,
                resolution_id = row.resolution_id,
            ));
            if !row.bad_pattern.is_empty() {
                body.push_str(&format!("  - bad: `{}`\n", row.bad_pattern));
            }
            if !row.good_pattern.is_empty() {
                body.push_str(&format!("  - good: `{}`\n", row.good_pattern));
            }
            body.push_str(&format!("  - resolution: {}\n", row.resolution));
        }
        body.push('\n');
    }
}

/// Writes `foreign-error-attenuation-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ForeignErrorAttenuationSummaryReporter;

impl ForeignErrorAttenuationSummaryReporter {
    pub const ID: &'static str = "foreign-error-attenuation-summary";
}

impl Reporter for ForeignErrorAttenuationSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = attenuation_rows(findings);
        let report = report_from_rows(&rows);
        let summary = build_workspace_foreign_error_attenuation_summary(&[report]);
        let body = render_summary(&summary);

        Ok(vec![Box::new(TextArtifact {
            name: "foreign-error-attenuation-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

#[instrument(level = "debug", skip(summary))]
fn render_summary(summary: &WorkspaceForeignErrorAttenuationSummary) -> String {
    let mut body = String::new();
    body.push_str("# Foreign error handling attenuation summary\n\n");
    body.push_str("---\n\n");

    let rate = summary
        .preservation_rate
        .map(|value| format!("{:.1}%", value * 100.0))
        .unwrap_or_else(|| "n/a".to_string());
    body.push_str(&format!(
        "Typed foreign sites: **{}** — preserved **{}**, chain breaks **{}**, pending infra **{}**, \
         neutral **{}**.\n\n",
        summary.typed_sites,
        summary.chain_preserved,
        summary.chain_breaks,
        summary.pending_infrastructure,
        summary.neutral,
    ));
    body.push_str(&format!(
        "**Preservation rate** (preserved / (preserved + chain breaks)): **{rate}**. \
         **Migration backlog** (chain breaks + pending infra): **{}**.\n\n",
        summary.migration_backlog,
    ));

    body.push_str("## Resolution strategies\n\n");
    for (resolution_id, count) in &summary.resolutions {
        body.push_str(&format!("- `{resolution_id}`: {count}\n"));
    }
    body.push('\n');

    body.push_str("## By foreign error type\n\n");
    body.push_str(
        "| Foreign error type | Preserved | Chain breaks | Pending infra | Total | Preservation rate | Primary resolution |",
    );
    body.push_str("\n| --- | ---: | ---: | ---: | ---: | ---: | --- |");
    for row in &summary.types {
        let rate = row
            .preservation_rate
            .map(|value| format!("{:.0}%", value * 100.0))
            .unwrap_or_else(|| "n/a".to_string());
        body.push_str(&format!(
            "\n| `{}` | {} | {} | {} | {} | {} | `{}` |",
            row.foreign_error_type,
            row.chain_preserved,
            row.chain_breaks,
            row.pending_infrastructure,
            row.total,
            rate,
            row.primary_resolution_id,
        ));
    }
    body
}
