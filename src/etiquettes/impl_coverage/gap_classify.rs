//! Impl gap classification with optional feature-probe and wrapper coverage context.

use crate::feature_probe::TypeFeatureProbe;
use crate::rustdoc::{
    TraitPrereqs, WrapperCoverage, coverage_provider_label, covered_indirectly,
    effective_missing_our_traits, indirect_elicit_complete, join_wrapper_paths,
};

use super::types::ImplGapKind;

/// Full assessment for one inventoried type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplGapAssessment {
    pub gap_kind: Option<ImplGapKind>,
    pub missing_our_traits: String,
    pub missing_external_traits: String,
    pub elicit_complete_gap: bool,
    pub feature_gated_external: bool,
    pub blocked_by_orphan_rule: bool,
    pub feature_owner_crate: String,
    pub candidate_unlock_features: String,
    pub coverage_provider: String,
    pub wrapper_paths: String,
    pub covered_indirectly: bool,
}

/// Classify a type's impl gap, mirroring elicit_doc's `assess_impl_entry` gap kinds.
pub fn assess_impl_gap(
    source_crate: &str,
    prereqs: &TraitPrereqs,
    feature_probe: Option<&TypeFeatureProbe>,
    lifetime_blocks_elicitation: bool,
    wrappers: Option<&[WrapperCoverage]>,
) -> ImplGapAssessment {
    if prereqs.elicit_complete {
        return covered_assessment(wrappers);
    }

    let candidate_unlock_features = feature_probe
        .map(|probe| probe.candidate_unlock_features.clone())
        .unwrap_or_default();
    let feature_owner_crate = feature_probe
        .map(|probe| probe.feature_crate.clone())
        .unwrap_or_else(|| source_crate.to_string());

    let missing_external = missing_external_traits(prereqs);
    let direct_missing_our = prereqs.missing_our_traits();
    let direct_our_traits_complete = direct_missing_our.is_empty();
    let wrapped_indirectly = covered_indirectly(wrappers);
    let missing_our = effective_missing_our_traits(&direct_missing_our, wrappers);
    let our_traits_complete = missing_our.is_empty();
    let can_be_direct = prereqs.can_be_direct();
    let feature_gated_external = !lifetime_blocks_elicitation
        && !can_be_direct
        && !candidate_unlock_features.is_empty()
        && feature_probe
            .and_then(|probe| probe.probed_prereqs.as_ref())
            .is_some_and(TraitPrereqs::can_be_direct);
    let blocked_by_orphan_rule =
        !lifetime_blocks_elicitation && !can_be_direct && !feature_gated_external;
    let indirect_complete = indirect_elicit_complete(wrappers);
    let elicit_complete_gap = can_be_direct && !indirect_complete;
    let coverage_provider = coverage_provider_label(
        direct_our_traits_complete,
        indirect_complete,
        our_traits_complete,
        wrapped_indirectly,
    );
    let wrapper_paths = join_wrapper_paths(wrappers);

    if indirect_complete {
        return ImplGapAssessment {
            gap_kind: None,
            missing_our_traits: String::new(),
            missing_external_traits: String::new(),
            elicit_complete_gap: false,
            feature_gated_external: false,
            blocked_by_orphan_rule: false,
            feature_owner_crate: String::new(),
            candidate_unlock_features: String::new(),
            coverage_provider,
            wrapper_paths,
            covered_indirectly: wrapped_indirectly,
        };
    }

    let gap_kind = if !our_traits_complete {
        Some(ImplGapKind::MissingOurTraits)
    } else if elicit_complete_gap {
        Some(ImplGapKind::ReadyForElicitComplete)
    } else if feature_gated_external {
        Some(ImplGapKind::FeatureGatedExternal)
    } else {
        None
    };

    ImplGapAssessment {
        gap_kind,
        missing_our_traits: missing_our.join("; "),
        missing_external_traits: missing_external.join(";"),
        elicit_complete_gap,
        feature_gated_external,
        blocked_by_orphan_rule,
        feature_owner_crate,
        candidate_unlock_features: if feature_gated_external {
            candidate_unlock_features.join(";")
        } else {
            String::new()
        },
        coverage_provider,
        wrapper_paths,
        covered_indirectly: wrapped_indirectly,
    }
}

fn covered_assessment(wrappers: Option<&[WrapperCoverage]>) -> ImplGapAssessment {
    ImplGapAssessment {
        gap_kind: None,
        missing_our_traits: String::new(),
        missing_external_traits: String::new(),
        elicit_complete_gap: false,
        feature_gated_external: false,
        blocked_by_orphan_rule: false,
        feature_owner_crate: String::new(),
        candidate_unlock_features: String::new(),
        coverage_provider: "direct".to_string(),
        wrapper_paths: join_wrapper_paths(wrappers),
        covered_indirectly: covered_indirectly(wrappers),
    }
}

fn missing_external_traits(prereqs: &TraitPrereqs) -> Vec<String> {
    [
        (prereqs.serialize, "Serialize"),
        (prereqs.deserialize, "Deserialize"),
        (prereqs.json_schema, "JsonSchema"),
    ]
    .into_iter()
    .filter(|(present, _)| !present)
    .map(|(_, name)| format!("{name}(absent)"))
    .collect()
}
