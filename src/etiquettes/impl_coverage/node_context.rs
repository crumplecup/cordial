use crate::enricher::{FeatureProbeEnricher, ProofHarnessEnricher, WrapperCoverageEnricher};
use crate::feature_probe::TypeFeatureProbe;
use crate::ir::NodeView;
use crate::rustdoc::WrapperCoverage;

use tracing::instrument;
#[instrument(level = "debug", skip(node))]
pub fn feature_probe_from_node(node: &dyn NodeView) -> Option<TypeFeatureProbe> {
    let feature_crate = node
        .attr(FeatureProbeEnricher::ATTR_CRATE)
        .and_then(|value| value.as_str())
        .map(str::to_string)?;
    let candidate_unlock_features = node
        .attr(FeatureProbeEnricher::ATTR_CANDIDATE_FEATURES)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    let probed_prereqs = node
        .attr(FeatureProbeEnricher::ATTR_PROBED_PREREQS)
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    Some(TypeFeatureProbe {
        feature_crate,
        candidate_unlock_features,
        probed_prereqs,
    })
}

#[instrument(level = "debug", skip(node))]
pub fn wrapper_coverage_from_node(node: &dyn NodeView) -> Option<Vec<WrapperCoverage>> {
    node.attr(WrapperCoverageEnricher::ATTR_WRAPPER_COVERAGE)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

#[instrument(level = "debug", skip(node))]
pub fn proof_test_from_node(node: &dyn NodeView) -> Option<String> {
    node.attr(ProofHarnessEnricher::ATTR_PROOF_TEST)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

#[instrument(level = "debug", skip(node))]
pub fn composition_test_from_node(node: &dyn NodeView) -> Option<String> {
    node.attr(ProofHarnessEnricher::ATTR_COMPOSITION_TEST)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}
