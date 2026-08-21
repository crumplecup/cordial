use std::collections::HashSet;

use crate::error::CordialResult;
use crate::hooks::{AssessView, Assessor};
use crate::ir::{type_trait_impls, type_trait_prereqs};
use crate::objects::{Disposition, Finding};
use crate::rustdoc::prereqs_from_trait_shorts;

use super::gap_classify::assess_impl_gap;
use super::node_context::{
    composition_test_from_node, feature_probe_from_node, proof_test_from_node,
    wrapper_coverage_from_node,
};
use super::types::{CoverageRule, ImplGapFinding};

use tracing::instrument;
#[derive(Debug, Default, Clone, Copy)]
pub struct ImplGapAssessor;

impl ImplGapAssessor {
    pub const ID: &'static str = "impl-gap-assessor";
}

impl Assessor for ImplGapAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        Self::ID
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> &[&str] {
        &["impl-coverage-gap"]
    }

    #[instrument(level = "trace", skip(self, view))]
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        let markers = view.markers;
        let ir = view.ir;

        let mut findings = Vec::new();
        let mut seen = HashSet::new();

        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
            let type_path = node
                .attr("qualified_path")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            if !seen.insert(type_path.clone()) {
                continue;
            }

            let prereqs = type_trait_prereqs(ir, &type_path)
                .unwrap_or_else(|| prereqs_from_trait_shorts(&type_trait_impls(ir, &type_path)));
            let feature_probe = feature_probe_from_node(&node);
            let wrapper_coverage = wrapper_coverage_from_node(&node);
            let wrappers = wrapper_coverage.as_deref();
            let lifetime_blocks = node
                .attr("lifetime_params")
                .and_then(|v| v.as_array())
                .is_some_and(|values| !values.is_empty());
            let assessment = assess_impl_gap(
                ir.crate_name(),
                &prereqs,
                feature_probe.as_ref(),
                lifetime_blocks,
                wrappers,
            );

            let proof_test = proof_test_from_node(&node).unwrap_or_else(|| "Missing".to_string());
            let composition_test =
                composition_test_from_node(&node).unwrap_or_else(|| "Missing".to_string());

            let Some(gap_kind) = assessment.gap_kind else {
                let disposition = if assessment.blocked_by_orphan_rule {
                    Disposition::Suppressed
                } else {
                    Disposition::Exemplar
                };
                findings.push(coverage_finding(
                    node_id,
                    CoverageFindingArgs {
                        crate_name: ir.crate_name(),
                        type_path: &type_path,
                        gap_kind: None,
                        assessment,
                        proof_test: &proof_test,
                        composition_test: &composition_test,
                        disposition,
                    },
                ));
                continue;
            };

            findings.push(coverage_finding(
                node_id,
                CoverageFindingArgs {
                    crate_name: ir.crate_name(),
                    type_path: &type_path,
                    gap_kind: Some(gap_kind),
                    assessment,
                    proof_test: &proof_test,
                    composition_test: &composition_test,
                    disposition: Disposition::Open,
                },
            ));
        }
        Ok(findings)
    }
}

/// Every fact needed to build one coverage finding, bundled so
/// [`coverage_finding`] takes one argument (plus the node identity)
/// instead of eight.
struct CoverageFindingArgs<'a> {
    crate_name: &'a str,
    type_path: &'a str,
    gap_kind: Option<super::types::ImplGapKind>,
    assessment: super::gap_classify::ImplGapAssessment,
    proof_test: &'a str,
    composition_test: &'a str,
    disposition: Disposition,
}

#[instrument(level = "debug", skip(node_id, args))]
fn coverage_finding(node_id: crate::ir::NodeId, args: CoverageFindingArgs<'_>) -> Box<dyn Finding> {
    Box::new(ImplGapFinding {
        rule: CoverageRule,
        disposition: args.disposition,
        anchor: crate::objects::NodeAnchor(node_id),
        crate_name: args.crate_name.to_string(),
        type_path: args.type_path.to_string(),
        gap_kind: args.gap_kind,
        missing_our_traits: args.assessment.missing_our_traits,
        missing_external_traits: args.assessment.missing_external_traits,
        elicit_complete_gap: args.assessment.elicit_complete_gap,
        proof_test: args.proof_test.to_string(),
        composition_test: args.composition_test.to_string(),
        feature_gated_external: args.assessment.feature_gated_external,
        feature_owner_crate: args.assessment.feature_owner_crate,
        candidate_unlock_features: args.assessment.candidate_unlock_features,
        coverage_provider: args.assessment.coverage_provider,
        wrapper_paths: args.assessment.wrapper_paths,
        covered_indirectly: args.assessment.covered_indirectly,
    })
}
