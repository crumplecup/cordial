use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct CfgScatterRow {
    crate_name: String,
    predicate: String,
    file: String,
    kinds: String,
    occurrences: String,
    sample: String,
    disposition: String,
}

impl CfgScatterRow {
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
            predicate: field("predicate"),
            file: field("file"),
            kinds: field("kinds"),
            occurrences: field("occurrences"),
            sample: field("sample"),
            disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn cfg_scatter_rows(findings: &[&dyn Finding]) -> Vec<CfgScatterRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "cfg_scatter")
        .map(|finding| CfgScatterRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn open_rows(rows: &[CfgScatterRow]) -> impl Iterator<Item = &CfgScatterRow> {
    rows.iter().filter(|row| row.disposition == "open")
}

#[instrument(level = "debug", skip(rows))]
fn sort_by_occurrences_desc(rows: &mut [&CfgScatterRow]) {
    rows.sort_by(|left, right| {
        right
            .occurrences
            .parse::<u32>()
            .unwrap_or(0)
            .cmp(&left.occurrences.parse::<u32>().unwrap_or(0))
            .then_with(|| left.file.cmp(&right.file))
            .then_with(|| left.predicate.cmp(&right.predicate))
    });
}

/// Writes `cfg-scatter.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgScatterCsvReporter;

impl CfgScatterCsvReporter {
    pub const ID: &'static str = "cfg-scatter-csv";
}

impl Reporter for CfgScatterCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let all_rows = cfg_scatter_rows(findings);
        let mut rows: Vec<_> = open_rows(&all_rows).collect();
        sort_by_occurrences_desc(&mut rows);

        let mut body = String::from("crate,predicate,file,kinds,occurrences,sample\n");
        for row in rows {
            body.push_str(&format!(
                "{},\"{}\",{},{},{},\"{}\"\n",
                row.crate_name,
                row.predicate.replace('"', "''"),
                row.file,
                row.kinds,
                row.occurrences,
                row.sample.replace('"', "''"),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "cfg-scatter.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `cfg-scatter.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgScatterChecklistReporter;

impl CfgScatterChecklistReporter {
    pub const ID: &'static str = "cfg-scatter-checklist";
}

impl Reporter for CfgScatterChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = cfg_scatter_rows(findings);
        let mut open: Vec<_> = open_rows(&rows).collect();
        sort_by_occurrences_desc(&mut open);

        let mut body = String::new();
        body.push_str("# Scattered `#[cfg(...)]` checklist\n\n");
        body.push_str(&format!("**Open items:** {}\n\n", open.len()));
        body.push_str(
            "Each row is one `#[cfg(...)]` predicate applied repeatedly to different item \
             kinds (functions, impls, imports, …) in the same file. Struct/enum field gating \
             alone is never listed here. Recommended fix: extract the gated items into their \
             own module and gate the whole `mod` declaration once instead.\n\n",
        );

        if !open.is_empty() {
            body.push_str(&format!("## `{}`\n\n", ir.crate_name()));
            for row in &open {
                body.push_str(&format!(
                    "- [ ] `{}` — `cfg({})` — kinds: `{}` — **{} occurrences**\n  - sample: {}\n",
                    row.file, row.predicate, row.kinds, row.occurrences, row.sample
                ));
            }
            body.push('\n');
        } else {
            body.push_str("_No scattered cfg predicates found._\n\n");
        }

        Ok(vec![Box::new(TextArtifact {
            name: "cfg-scatter.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `cfg-scatter-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CfgScatterSummaryReporter;

impl CfgScatterSummaryReporter {
    pub const ID: &'static str = "cfg-scatter-summary";
}

impl Reporter for CfgScatterSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = cfg_scatter_rows(findings);
        let open: Vec<_> = open_rows(&rows).collect();
        let total_occurrences: u32 = open
            .iter()
            .filter_map(|row| row.occurrences.parse::<u32>().ok())
            .sum();
        let files_affected = open
            .iter()
            .map(|row| row.file.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .len();

        let mut body = String::new();
        body.push_str("# Scattered cfg summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{}** scattered predicates across **{files_affected}** files, \
             **{total_occurrences}** total gated sites.\n\n",
            open.len()
        ));
        body.push_str("| Crate | Scattered predicates | Files affected | Total gated sites |\n");
        body.push_str("| --- | ---: | ---: | ---: |\n");
        body.push_str(&format!(
            "| `{}` | {} | {files_affected} | {total_occurrences} |\n",
            ir.crate_name(),
            open.len()
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "cfg-scatter-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
