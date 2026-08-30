use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct CoverageRow {
    crate_name: String,
    type_path: String,
    gap_kind: String,
    missing_our_traits: String,
    missing_external_traits: String,
    elicit_complete_gap: String,
    proof_test: String,
    composition_test: String,
    feature_gated_external: String,
    feature_owner_crate: String,
    candidate_unlock_features: String,
    coverage_provider: String,
    wrapper_paths: String,
    covered_indirectly: String,
    disposition: String,
}

#[instrument(level = "debug", skip(findings))]
fn coverage_rows(findings: &[&dyn Finding]) -> Vec<CoverageRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "impl-coverage")
        .map(|finding| {
            let mut sink = MapFindingSink::default();
            finding.emit(&mut sink);
            let field = |name: &str| {
                sink.fields
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default()
            };
            CoverageRow {
                crate_name: field("crate"),
                type_path: field("type_path"),
                gap_kind: field("gap_kind"),
                missing_our_traits: field("missing_our_traits"),
                missing_external_traits: field("missing_external_traits"),
                elicit_complete_gap: field("elicit_complete_gap"),
                proof_test: field("proof_test"),
                composition_test: field("composition_test"),
                feature_gated_external: field("feature_gated_external"),
                feature_owner_crate: field("feature_owner_crate"),
                candidate_unlock_features: field("candidate_unlock_features"),
                coverage_provider: field("coverage_provider"),
                wrapper_paths: field("wrapper_paths"),
                covered_indirectly: field("covered_indirectly"),
                disposition: finding.disposition().to_string(),
            }
        })
        .collect()
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ImplCoverageCsvReporter;

impl ImplCoverageCsvReporter {
    pub const ID: &'static str = "impl-coverage-csv";
}

impl Reporter for ImplCoverageCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from(
            "crate,type_path,gap_kind,missing_our_traits,missing_external_traits,elicit_complete_gap,proof_test,composition_test,disposition\n",
        );
        for row in coverage_rows(findings) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.type_path),
                csv_field(&row.gap_kind),
                csv_field(&row.missing_our_traits),
                csv_field(&row.missing_external_traits),
                csv_field(&row.elicit_complete_gap),
                csv_field(&row.proof_test),
                csv_field(&row.composition_test),
                csv_field(&row.disposition),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "impl-coverage.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ImplGapsCsvReporter;

impl ImplGapsCsvReporter {
    pub const ID: &'static str = "impl-gaps-csv";
}

impl Reporter for ImplGapsCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let mut body = String::from(
            "crate,type_path,gap_kind,missing_our_traits,missing_external_traits,elicit_complete_gap,feature_gated_external,feature_owner_crate,candidate_unlock_features,coverage_provider,wrapper_paths,covered_indirectly\n",
        );
        for row in coverage_rows(findings)
            .into_iter()
            .filter(|row| row.disposition == "open")
        {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.type_path),
                csv_field(&row.gap_kind),
                csv_field(&row.missing_our_traits),
                csv_field(&row.missing_external_traits),
                csv_field(&row.elicit_complete_gap),
                csv_field(&row.feature_gated_external),
                csv_field(&row.feature_owner_crate),
                csv_field(&row.candidate_unlock_features),
                csv_field(&row.coverage_provider),
                csv_field(&row.wrapper_paths),
                csv_field(&row.covered_indirectly),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "gaps-impl.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ImplChecklistReporter;

impl ImplChecklistReporter {
    pub const ID: &'static str = "impl-checklist";
}

impl Reporter for ImplChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows: Vec<_> = coverage_rows(findings)
            .into_iter()
            .filter(|row| row.disposition == "open")
            .collect();
        let mut body = String::new();
        body.push_str("# Impl coverage checklist\n\n");
        body.push_str(&format!("**Open gaps:** {}\n\n", rows.len()));
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));
        for row in rows {
            body.push_str(&format!(
                "- [ ] `{}` — **{}** (our: {}; external: {})\n",
                row.type_path, row.gap_kind, row.missing_our_traits, row.missing_external_traits,
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "impl-coverage.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
