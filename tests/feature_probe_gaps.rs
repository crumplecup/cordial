//! Feature probe gap classification parity with elicit_doc.

use cordial::rustdoc::TraitPrereqs;
use cordial::testing::TypeFeatureProbe;
use cordial::testing::{ImplGapKind, assess_impl_gap};
use miette::{IntoDiagnostic, WrapErr};

fn prereqs_with_missing_external() -> TraitPrereqs {
    TraitPrereqs {
        serialize: false,
        deserialize: false,
        json_schema: false,
        elicitation_trait: true,
        elicit_introspect: false,
        elicit_spec: true,
        elicit_prompt_tree: false,
        to_code_literal: true,
        ..TraitPrereqs::default()
    }
}

#[test]
fn flags_feature_gated_external_when_probe_unlocks_direct_impl() {
    cordial::init_tracing();
    let prereqs = prereqs_with_missing_external();
    let probe = TypeFeatureProbe {
        feature_crate: "reqwest".to_string(),
        candidate_unlock_features: vec!["json".to_string()],
        probed_prereqs: Some(TraitPrereqs {
            serialize: true,
            deserialize: true,
            json_schema: true,
            ..prereqs.clone()
        }),
    };

    let assessment = assess_impl_gap("reqwest", &prereqs, Some(&probe), false, None);
    assert_eq!(assessment.gap_kind, Some(ImplGapKind::MissingOurTraits));
    assert!(assessment.feature_gated_external);
    assert!(!assessment.blocked_by_orphan_rule);
    assert_eq!(assessment.feature_owner_crate, "reqwest");
    assert_eq!(assessment.candidate_unlock_features, "json");
}

#[test]
fn classifies_pure_feature_gated_external_when_our_traits_complete() {
    cordial::init_tracing();
    let prereqs = TraitPrereqs {
        serialize: false,
        deserialize: false,
        json_schema: false,
        elicitation_trait: true,
        elicit_introspect: true,
        elicit_spec: true,
        elicit_prompt_tree: true,
        to_code_literal: true,
        ..TraitPrereqs::default()
    };
    let probe = TypeFeatureProbe {
        feature_crate: "reqwest".to_string(),
        candidate_unlock_features: vec!["json".to_string()],
        probed_prereqs: Some(TraitPrereqs {
            serialize: true,
            deserialize: true,
            json_schema: true,
            ..prereqs.clone()
        }),
    };

    let assessment = assess_impl_gap("reqwest", &prereqs, Some(&probe), false, None);
    assert_eq!(assessment.gap_kind, Some(ImplGapKind::FeatureGatedExternal));
    assert!(assessment.feature_gated_external);
    assert!(!assessment.blocked_by_orphan_rule);
}

#[test]
fn marks_externally_blocked_when_no_probe_unlock() {
    cordial::init_tracing();
    let prereqs = TraitPrereqs {
        serialize: false,
        deserialize: false,
        json_schema: false,
        elicitation_trait: true,
        elicit_introspect: true,
        elicit_spec: true,
        elicit_prompt_tree: true,
        to_code_literal: true,
        ..TraitPrereqs::default()
    };

    let assessment = assess_impl_gap("reqwest", &prereqs, None, false, None);
    assert!(assessment.gap_kind.is_none());
    assert!(assessment.blocked_by_orphan_rule);
    assert!(!assessment.feature_gated_external);
}

#[test]
fn collect_dep_serde_features_finds_unlock_candidates_for_url_fixture() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/parity/workspaces/minimal-workspace");
    let (available, expanded) =
        cordial::testing::collect_dep_serde_features(&workspace, "url", &["serde"], false)
            .into_diagnostic()
            .wrap_err("dep serde features")?;
    assert!(available.contains(&"serde".to_string()));
    assert!(expanded.contains(&"serde".to_string()));
    let candidates: Vec<_> = available
        .into_iter()
        .filter(|feature| !expanded.contains(feature))
        .collect();
    assert!(
        candidates.is_empty(),
        "minimal url fixture should have no unlock candidates with serde active: {candidates:?}"
    );
    Ok(())
}
