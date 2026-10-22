//! Render the internal error chain checklist artifact.

use std::collections::{BTreeMap, HashSet};

use tracing::instrument;

use crate::error::CordialResult;
use crate::etiquette::{finding_field, open_findings};
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, TextArtifact};

use super::{
    InternalErrorChainRow, compliance_rows, crate_names, internal_error_chain_rows, type_graph_rows,
};

/// Writes `internal-error-chain.checklist.md`.
#[derive(Debug, Default, Clone, Copy)]
pub struct InternalErrorChainChecklistReporter;

impl InternalErrorChainChecklistReporter {
    pub const ID: &'static str = "internal-error-chain-checklist";
}

impl Reporter for InternalErrorChainChecklistReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn Artifact>>> {
        let findings = view.findings();

        let rows = internal_error_chain_rows(findings);
        let type_nodes: Vec<_> = type_graph_rows(&rows).collect();
        let compliance: Vec<_> = compliance_rows(&rows).collect();
        let attenuation_sites: HashSet<_> = open_findings(findings)
            .filter(|finding| finding.rule().category() == "foreign_error_attenuation")
            .filter(|finding| {
                matches!(
                    finding_field(*finding, "handling_class").as_deref(),
                    Some("ERROR-HANDLING-CHAIN-BREAK" | "ERROR-HANDLING-PENDING-INFRA")
                )
            })
            .map(|finding| {
                (
                    finding_field(finding, "file").unwrap_or_default(),
                    finding_field(finding, "line").unwrap_or_default(),
                )
            })
            .collect();

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
             carry `file`/`line` copied from `Location::caller()` according to \
             `[internal_error_chain] field_strategy` — do not store \
             `&'static Location`. Capture that location in a custom \
             `#[track_caller] fn new` — do not pass file/line as arguments. Parent \
             constructors that wrap a source must also be \
             `#[track_caller]` so the call site is preserved. The parent error holds a \
             `*Kind` enum using the configured strategy; every Kind variant is a native source that implements `Error`. \
             Types that implement `Error` anywhere under `src/` are the catalog — \
             sources may live next to their call site. When the crate has a library \
             and a binary, clap types and dispatch live in the library (`Cli::act`); \
             `main` only parses and converts the umbrella error with miette.\n\n",
        );

        let all_rows: Vec<&InternalErrorChainRow> = rows.iter().collect();
        for crate_name in crate_names(&all_rows) {
            let crate_type_nodes: Vec<_> = type_nodes
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            let crate_compliance: Vec<_> = compliance
                .iter()
                .copied()
                .filter(|row| row.crate_name == crate_name)
                .collect();
            body.push_str(&format!("## `{crate_name}`\n\n"));

            if !crate_type_nodes.is_empty() {
                body.push_str("### Type graph\n\n");
                let mut by_class: BTreeMap<String, Vec<&InternalErrorChainRow>> = BTreeMap::new();
                for row in &crate_type_nodes {
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

            if !crate_compliance.is_empty() {
                body.push_str("### Compliance violations\n\n");
                let mut by_rule: BTreeMap<String, Vec<&InternalErrorChainRow>> = BTreeMap::new();
                for row in &crate_compliance {
                    by_rule.entry(row.rule_id.clone()).or_default().push(row);
                }
                for (rule_id, entries) in by_rule {
                    body.push_str(&format!("#### {rule_id}\n\n"));
                    for entry in entries {
                        let already_listed =
                            attenuation_sites.contains(&(entry.file.clone(), entry.line.clone()));
                        let item_prefix = if already_listed { "- " } else { "- [ ] " };
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
                            "{item_prefix}`{}` — `{}:{}` — foreign `{foreign}` — constructor `{constructor}` — `{}`{}\n",
                            entry.context,
                            entry.file,
                            entry.line,
                            entry.snippet,
                            if already_listed { " — also in foreign-error-attenuation.checklist.md" } else { "" }
                        ));
                    }
                    body.push('\n');
                }
            }
        }

        Ok(vec![Box::new(TextArtifact::new(
            "internal-error-chain.checklist.md".to_string(),
            "text/markdown".to_string(),
            body,
        ))])
    }
}
