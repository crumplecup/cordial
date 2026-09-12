//! Wrapper coverage integration with impl gap classification.

use cordial::rustdoc::{
    WrapperCoverage, WrapperCoverageMap, build_wrapper_coverage_map, lookup_wrapper_coverage,
};
use cordial::testing::assess_impl_gap;
use cordial::{ElicitCompleteSet, TraitPrereqs};

fn prereqs_from_trait_shorts(traits: &[&str]) -> TraitPrereqs {
    let mut prereqs = TraitPrereqs::default();
    for trait_short in traits {
        prereqs.apply_trait_short(trait_short);
    }
    prereqs
}

#[test]
fn wrapper_elicit_complete_suppresses_foreign_gap() {
    cordial::init_tracing();
    let foreign = "demo::Foreign";
    let wrapper = "demo::ForeignWrapper";
    let mut wrapper_prereqs = std::collections::HashMap::new();
    wrapper_prereqs.insert(
        wrapper.to_string(),
        prereqs_from_trait_shorts(&["ElicitComplete"]),
    );
    let complete = ElicitCompleteSet::new(
        std::collections::HashSet::from([wrapper.to_string()]),
        std::collections::HashSet::new(),
    );
    let map = build_wrapper_coverage_map(
        &[(foreign.to_string(), wrapper.to_string())],
        &complete,
        &wrapper_prereqs,
    );
    let wrappers = lookup_wrapper_coverage(&map, foreign).map(Vec::as_slice);

    let prereqs = TraitPrereqs::default();
    let assessment = assess_impl_gap("demo", &prereqs, None, false, wrappers);
    assert!(
        assessment.gap_kind().is_none(),
        "wrapper-complete foreign type is covered"
    );
    assert!(assessment.wrapper_paths().contains(wrapper));
    assert!(assessment.covered_indirectly());
}

#[test]
fn partial_wrapper_prereqs_credit_indirect_our_traits() {
    cordial::init_tracing();
    let foreign = "demo::Foreign";
    let wrapper = "demo::PartialWrapper";
    let mut wrapper_prereqs = std::collections::HashMap::new();
    wrapper_prereqs.insert(
        wrapper.to_string(),
        prereqs_from_trait_shorts(&[
            "Elicitation",
            "ElicitIntrospect",
            "ElicitSpec",
            "ElicitPromptTree",
            "ToCodeLiteral",
        ]),
    );
    let map = build_wrapper_coverage_map(
        &[(foreign.to_string(), wrapper.to_string())],
        &ElicitCompleteSet::default(),
        &wrapper_prereqs,
    );
    let wrappers = lookup_wrapper_coverage(&map, foreign).map(Vec::as_slice);

    let prereqs = TraitPrereqs::default();
    let assessment = assess_impl_gap("demo", &prereqs, None, false, wrappers);
    assert!(
        assessment.gap_kind().is_none(),
        "our traits satisfied via wrapper"
    );
    assert!(assessment.covered_indirectly());
    assert_eq!(assessment.coverage_provider(), "wrapper");
    assert!(assessment.blocked_by_orphan_rule());
}

#[test]
fn lookup_wrapper_coverage_falls_back_to_bare_name() {
    cordial::init_tracing();
    let mut map = WrapperCoverageMap::new();
    map.insert(
        "chrono::naive::date::NaiveDate".to_string(),
        vec![WrapperCoverage::new(
            "elicitation::NaiveDateCoat".to_string(),
            false,
            TraitPrereqs::default(),
        )],
    );
    assert!(lookup_wrapper_coverage(&map, "chrono::NaiveDate").is_some());
}
