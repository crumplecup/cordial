//! Render the internal error chain summary artifact.

use tracing::instrument;

use crate::error::CordialResult;
use crate::hooks::{RenderView, Reporter};
use crate::objects::{Artifact, TextArtifact};

use super::super::types::InternalErrorNodeClass;
use super::{
    InternalErrorChainRow, class_counts, compliance_counts, compliance_rows, crate_names,
    internal_error_chain_rows, type_graph_rows,
};

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
        let findings = view.findings();

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
            field_strategy_violations,
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
             **{field_strategy_violations}** field-strategy, \
             **{track_caller_violations}** track-caller, **{architecture_violations}** architecture).\n\n"
        ));
        body.push_str(
            "| Crate | Nodes | Leaves | Links | Bridges | Violations | Stringify | Discard | Source shape | Field strategy | Track caller | Architecture |\n",
        );
        body.push_str(
            "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
        );
        let all_rows: Vec<&InternalErrorChainRow> = rows.iter().collect();
        for crate_name in crate_names(&all_rows) {
            let crate_rows: Vec<InternalErrorChainRow> = rows
                .iter()
                .filter(|row| row.crate_name == crate_name)
                .cloned()
                .collect();
            let crate_counts = class_counts(&crate_rows);
            let crate_type_nodes = type_graph_rows(&crate_rows).count();
            let crate_leaves = crate_counts
                .get(InternalErrorNodeClass::InternalLeaf.as_str())
                .copied()
                .unwrap_or(0);
            let crate_links = crate_counts
                .get(InternalErrorNodeClass::InternalLink.as_str())
                .copied()
                .unwrap_or(0)
                + crate_counts
                    .get(InternalErrorNodeClass::UmbrellaWrapper.as_str())
                    .copied()
                    .unwrap_or(0);
            let crate_bridges = crate_counts
                .get(InternalErrorNodeClass::ForeignBridge.as_str())
                .copied()
                .unwrap_or(0);
            let crate_compliance_findings = compliance_rows(&crate_rows).count();
            let (
                crate_stringify,
                crate_discard,
                crate_source_shape,
                crate_field_strategy,
                crate_track_caller,
                crate_architecture,
            ) = compliance_counts(&crate_rows);
            body.push_str(&format!(
                "| `{crate_name}` | {crate_type_nodes} | {crate_leaves} | {crate_links} | {crate_bridges} | \
                 {crate_compliance_findings} | {crate_stringify} | {crate_discard} | \
                 {crate_source_shape} | {crate_field_strategy} | {crate_track_caller} | {crate_architecture} |\n"
            ));
        }
        body.push_str(&format!(
            "\n| **Total** | **{type_nodes}** | **{internal_leaves}** | **{internal_links}** | **{foreign_bridges}** | \
             **{compliance_findings}** | **{stringify_violations}** | **{discard_violations}** | \
             **{source_shape_violations}** | **{field_strategy_violations}** | **{track_caller_violations}** | **{architecture_violations}** |\n"
        ));

        Ok(vec![Box::new(TextArtifact::new(
            "internal-error-chain-summary.md".to_string(),
            "text/markdown".to_string(),
            body,
        ))])
    }
}
