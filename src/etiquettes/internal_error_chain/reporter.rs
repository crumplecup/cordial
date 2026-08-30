use std::collections::BTreeMap;

use crate::csv_row::csv_field;
use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, Finding, MapFindingSink, TextArtifact};

use super::types::{InternalErrorComplianceId, InternalErrorNodeClass, InternalErrorRecordKind};

use tracing::instrument;
#[derive(Debug, Default, Clone)]
struct InternalErrorChainRow {
    crate_name: String,
    record_kind: String,
    rule_id: String,
    context: String,
    type_path: String,
    node_class: String,
    source_target: String,
    reaches_foreign: String,
    chain_depth: String,
    foreign_error_type: String,
    internal_constructor: String,
    file: String,
    line: String,
    snippet: String,
    _disposition: String,
}

impl InternalErrorChainRow {
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
            record_kind: field("record_kind"),
            rule_id: field("rule_id"),
            context: field("context"),
            type_path: field("type_path"),
            node_class: field("node_class"),
            source_target: field("source_target"),
            reaches_foreign: field("reaches_foreign"),
            chain_depth: field("chain_depth"),
            foreign_error_type: field("foreign_error_type"),
            internal_constructor: field("internal_constructor"),
            file: field("file"),
            line: field("line"),
            snippet: field("snippet"),
            _disposition: finding.disposition().to_string(),
        }
    }
}

#[instrument(level = "debug", skip(findings))]
fn internal_error_chain_rows(findings: &[&dyn Finding]) -> Vec<InternalErrorChainRow> {
    findings
        .iter()
        .filter(|finding| finding.rule().category() == "internal_error_chain")
        .map(|finding| InternalErrorChainRow::from_finding(*finding))
        .collect()
}

#[instrument(level = "debug", skip(rows))]
fn type_graph_rows(rows: &[InternalErrorChainRow]) -> impl Iterator<Item = &InternalErrorChainRow> {
    rows.iter()
        .filter(|row| row.record_kind == InternalErrorRecordKind::TypeGraph.as_str())
}

#[instrument(level = "debug", skip(rows))]
fn compliance_rows(rows: &[InternalErrorChainRow]) -> impl Iterator<Item = &InternalErrorChainRow> {
    rows.iter()
        .filter(|row| row.record_kind == InternalErrorRecordKind::Compliance.as_str())
}

#[instrument(level = "debug", skip(rows))]
fn class_counts(rows: &[InternalErrorChainRow]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in type_graph_rows(rows) {
        if row.node_class.is_empty() {
            continue;
        }
        *counts.entry(row.node_class.clone()).or_default() += 1;
    }
    counts
}

#[instrument(level = "debug", skip(rows))]
fn compliance_counts(rows: &[InternalErrorChainRow]) -> (usize, usize, usize, usize, usize) {
    let mut stringify = 0usize;
    let mut discard = 0usize;
    let mut source_shape = 0usize;
    let mut track_caller = 0usize;
    let mut architecture = 0usize;
    for row in compliance_rows(rows) {
        match row.rule_id.as_str() {
            id if id == InternalErrorComplianceId::StringifyForeign001.as_str() => {
                stringify += 1;
            }
            id if id == InternalErrorComplianceId::DiscardTyped001.as_str() => discard += 1,
            id if id == InternalErrorComplianceId::SourceShape001.as_str() => source_shape += 1,
            id if id == InternalErrorComplianceId::SourceTrackCaller001.as_str() => {
                track_caller += 1;
            }
            id if id == InternalErrorComplianceId::ArchParent001.as_str()
                || id == InternalErrorComplianceId::ArchKindBox001.as_str()
                || id == InternalErrorComplianceId::ArchKindVariant001.as_str()
                || id == InternalErrorComplianceId::ArchOrphanSource001.as_str() =>
            {
                architecture += 1;
            }
            _ => {}
        }
    }
    (stringify, discard, source_shape, track_caller, architecture)
}

/// Writes `internal-error-type-graph.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalErrorTypeGraphCsvReporter;

impl InternalErrorTypeGraphCsvReporter {
    pub const ID: &'static str = "internal-error-type-graph-csv";
}

impl Reporter for InternalErrorTypeGraphCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = internal_error_chain_rows(findings);
        let mut body = String::from(
            "crate,type_path,node_class,probe_id,source_target,reaches_foreign,chain_depth,file,line,snippet\n",
        );
        for row in type_graph_rows(&rows) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.type_path),
                csv_field(&row.node_class),
                csv_field(&row.rule_id),
                csv_field(&row.source_target),
                csv_field(&row.reaches_foreign),
                csv_field(&row.chain_depth),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "internal-error-type-graph.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `internal-error-compliance.csv`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalErrorComplianceCsvReporter;

impl InternalErrorComplianceCsvReporter {
    pub const ID: &'static str = "internal-error-compliance-csv";
}

impl Reporter for InternalErrorComplianceCsvReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;

        let rows = internal_error_chain_rows(findings);
        let mut body = String::from(
            "crate,rule_id,foreign_error_type,internal_constructor,context,file,line,snippet\n",
        );
        for row in compliance_rows(&rows) {
            body.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                csv_field(&row.crate_name),
                csv_field(&row.rule_id),
                csv_field(&row.foreign_error_type),
                csv_field(&row.internal_constructor),
                csv_field(&row.context),
                csv_field(&row.file),
                csv_field(&row.line),
                csv_field(&row.snippet),
            ));
        }
        Ok(vec![Box::new(TextArtifact {
            name: "internal-error-compliance.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}

/// Writes `internal-error-chain.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalErrorChainChecklistReporter;

impl InternalErrorChainChecklistReporter {
    pub const ID: &'static str = "internal-error-chain-checklist";
}

impl Reporter for InternalErrorChainChecklistReporter {
    fn id(&self) -> &str {
        Self::ID
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = internal_error_chain_rows(findings);
        let type_nodes: Vec<_> = type_graph_rows(&rows).collect();
        let compliance: Vec<_> = compliance_rows(&rows).collect();

        let mut body = String::new();
        body.push_str("# Internal error chain checklist\n\n");
        body.push_str(&format!(
            "**Type graph nodes:** {} — **Compliance violations:** {}\n\n",
            type_nodes.len(),
            compliance.len()
        ));
        body.push_str(
            "Static inventory of crate error types (internal leaves vs links vs foreign bridges) \
             and call sites that stringify or discard typed foreign errors. \
             Source wrappers that hold a foreign error must keep it in `source` and \
             carry owned `file`/`line` copied from `Location::caller()` — do not store \
             `&'static Location`. Capture that location in a custom \
             `#[track_caller] fn new` — do not pass file/line as arguments. Parent \
             constructors that wrap a source must also be \
             `#[track_caller]` so the call site is preserved. The parent error boxes a \
             `*Kind` enum; every Kind variant is a native source that implements `Error`. \
             Types that implement `Error` anywhere under `src/` are the catalog — \
             sources may live next to their call site. When the crate has a library \
             and a binary, clap types and dispatch live in the library (`Cli::act`); \
             `main` only parses and converts the umbrella error with miette.\n\n",
        );
        body.push_str(&format!("## `{}`\n\n", ir.crate_name()));

        if !type_nodes.is_empty() {
            body.push_str("### Type graph\n\n");
            let mut by_class: BTreeMap<String, Vec<&InternalErrorChainRow>> = BTreeMap::new();
            for row in &type_nodes {
                by_class
                    .entry(row.node_class.clone())
                    .or_default()
                    .push(row);
            }
            for (class, entries) in by_class {
                body.push_str(&format!("#### {class}\n\n"));
                for entry in entries {
                    let target = if entry.source_target.is_empty() {
                        "—"
                    } else {
                        &entry.source_target
                    };
                    body.push_str(&format!(
                        "- [x] `{}` → `{target}` (depth {}, foreign={}) — `{}:{}`\n",
                        entry.type_path,
                        entry.chain_depth,
                        entry.reaches_foreign,
                        entry.file,
                        entry.line
                    ));
                }
                body.push('\n');
            }
        }

        if !compliance.is_empty() {
            body.push_str("### Compliance violations\n\n");
            let mut by_rule: BTreeMap<String, Vec<&InternalErrorChainRow>> = BTreeMap::new();
            for row in &compliance {
                by_rule.entry(row.rule_id.clone()).or_default().push(row);
            }
            for (rule_id, entries) in by_rule {
                body.push_str(&format!("#### {rule_id}\n\n"));
                for entry in entries {
                    let foreign = if entry.foreign_error_type.is_empty() {
                        "—"
                    } else {
                        &entry.foreign_error_type
                    };
                    let constructor = if entry.internal_constructor.is_empty() {
                        "—"
                    } else {
                        &entry.internal_constructor
                    };
                    body.push_str(&format!(
                        "- [ ] `{}` — `{}:{}` — foreign `{foreign}` — constructor `{constructor}` — `{}`\n",
                        entry.context, entry.file, entry.line, entry.snippet
                    ));
                }
                body.push('\n');
            }
        }

        Ok(vec![Box::new(TextArtifact {
            name: "internal-error-chain.checklist.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}

/// Writes `internal-error-chain-summary.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalErrorChainSummaryReporter;

impl InternalErrorChainSummaryReporter {
    pub const ID: &'static str = "internal-error-chain-summary";
}

impl Reporter for InternalErrorChainSummaryReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings;
        let ir = view.ir;

        let rows = internal_error_chain_rows(findings);
        let counts = class_counts(&rows);
        let type_nodes = type_graph_rows(&rows).count();
        let internal_leaves = counts
            .get(InternalErrorNodeClass::InternalLeaf.as_str())
            .copied()
            .unwrap_or(0);
        let internal_links = counts
            .get(InternalErrorNodeClass::InternalLink.as_str())
            .copied()
            .unwrap_or(0)
            + counts
                .get(InternalErrorNodeClass::UmbrellaWrapper.as_str())
                .copied()
                .unwrap_or(0);
        let foreign_bridges = counts
            .get(InternalErrorNodeClass::ForeignBridge.as_str())
            .copied()
            .unwrap_or(0);
        let compliance_findings = compliance_rows(&rows).count();
        let (
            stringify_violations,
            discard_violations,
            source_shape_violations,
            track_caller_violations,
            architecture_violations,
        ) = compliance_counts(&rows);

        let mut body = String::new();
        body.push_str("# Internal error chain summary\n\n");
        body.push_str("---\n\n");
        body.push_str(&format!(
            "Workspace totals: **{type_nodes}** type nodes — **{internal_leaves}** internal leaves, \
             **{internal_links}** internal links, **{foreign_bridges}** foreign bridges — \
             **{compliance_findings}** compliance violations (**{stringify_violations}** stringify, \
             **{discard_violations}** discard, **{source_shape_violations}** source-shape, \
             **{track_caller_violations}** track-caller, **{architecture_violations}** architecture).\n\n"
        ));
        body.push_str(
            "| Crate | Nodes | Leaves | Links | Bridges | Violations | Stringify | Discard | Source shape | Track caller | Architecture |\n",
        );
        body.push_str(
            "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
        );
        body.push_str(&format!(
            "| `{}` | {type_nodes} | {internal_leaves} | {internal_links} | {foreign_bridges} | \
             {compliance_findings} | {stringify_violations} | {discard_violations} | \
             {source_shape_violations} | {track_caller_violations} | {architecture_violations} |\n\n",
            ir.crate_name()
        ));

        Ok(vec![Box::new(TextArtifact {
            name: "internal-error-chain-summary.md".to_string(),
            media_type: "text/markdown".to_string(),
            body,
        })])
    }
}
