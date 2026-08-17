//! Wrapper coverage integration with impl gap classification.

use cordial::rustdoc::{
    WrapperCoverage, WrapperCoverageMap, build_wrapper_coverage_map, lookup_wrapper_coverage,
};
use cordial::testing::assess_impl_gap;
use cordial::{ElicitCompleteSet, TraitPrereqs};

#[test]
fn wrapper_elicit_complete_suppresses_foreign_gap() {
    let foreign = "demo::Foreign";
    let wrapper = "demo::ForeignWrapper";
    let mut wrapper_prereqs = std::collections::HashMap::new();
    wrapper_prereqs.insert(
        wrapper.to_string(),
        TraitPrereqs {
            elicit_complete: true,
            ..TraitPrereqs::default()
        },
    );
    let mut complete = ElicitCompleteSet::default();
    complete.concrete.insert(wrapper.to_string());
    let map = build_wrapper_coverage_map(
        &[(foreign.to_string(), wrapper.to_string())],
        &complete,
        &wrapper_prereqs,
    );
    let wrappers = lookup_wrapper_coverage(&map, foreign).map(Vec::as_slice);

    let prereqs = TraitPrereqs {
        serialize: false,
        deserialize: false,
        json_schema: false,
        ..TraitPrereqs::default()
    };
    let assessment = assess_impl_gap("demo", &prereqs, None, false, wrappers);
    assert!(
        assessment.gap_kind.is_none(),
        "wrapper-complete foreign type is covered"
    );
    assert!(assessment.wrapper_paths.contains(wrapper));
    assert!(assessment.covered_indirectly);
}

#[test]
fn partial_wrapper_prereqs_credit_indirect_our_traits() {
    let foreign = "demo::Foreign";
    let wrapper = "demo::PartialWrapper";
    let mut wrapper_prereqs = std::collections::HashMap::new();
    wrapper_prereqs.insert(
        wrapper.to_string(),
        TraitPrereqs {
            elicitation_trait: true,
            elicit_introspect: true,
            elicit_spec: true,
            elicit_prompt_tree: true,
            to_code_literal: true,
            ..TraitPrereqs::default()
        },
    );
    let map = build_wrapper_coverage_map(
        &[(foreign.to_string(), wrapper.to_string())],
        &ElicitCompleteSet::default(),
        &wrapper_prereqs,
    );
    let wrappers = lookup_wrapper_coverage(&map, foreign).map(Vec::as_slice);

    let prereqs = TraitPrereqs {
        serialize: false,
        deserialize: false,
        json_schema: false,
        ..TraitPrereqs::default()
    };
    let assessment = assess_impl_gap("demo", &prereqs, None, false, wrappers);
    assert!(
        assessment.gap_kind.is_none(),
        "our traits satisfied via wrapper"
    );
    assert!(assessment.covered_indirectly);
    assert_eq!(assessment.coverage_provider, "wrapper");
    assert!(assessment.blocked_by_orphan_rule);
}

#[test]
fn lookup_wrapper_coverage_falls_back_to_bare_name() {
    let mut map = WrapperCoverageMap::new();
    map.insert(
        "chrono::naive::date::NaiveDate".to_string(),
        vec![WrapperCoverage {
            wrapper_path: "elicitation::NaiveDateCoat".to_string(),
            wrapper_elicit_complete: false,
            wrapper_prereqs: TraitPrereqs::default(),
        }],
    );
    assert!(lookup_wrapper_coverage(&map, "chrono::NaiveDate").is_some());
}
