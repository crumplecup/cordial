//! Wrapper coverage map — foreign types covered via elicitation-owned trenchcoats.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::TraitPrereqs;
use super::elicit_complete::ElicitCompleteSet;

/// Coverage provided by one elicitation-owned wrapper for a foreign type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrapperCoverage {
    pub wrapper_path: String,
    pub wrapper_elicit_complete: bool,
    pub wrapper_prereqs: TraitPrereqs,
}

pub type WrapperCoverageMap = HashMap<String, Vec<WrapperCoverage>>;

pub fn lookup_wrapper_coverage<'a>(
    map: &'a WrapperCoverageMap,
    type_path: &str,
) -> Option<&'a Vec<WrapperCoverage>> {
    if let Some(wrappers) = map.get(type_path) {
        return Some(wrappers);
    }
    let bare_name = type_path.rsplit("::").next().unwrap_or("");
    let crate_prefix = type_path.split("::").next().unwrap_or("");
    if bare_name.is_empty() || crate_prefix.is_empty() || bare_name == crate_prefix {
        return None;
    }
    let suffix = format!("::{bare_name}");
    map.iter()
        .find(|(key, _)| key.ends_with(&suffix) && key.split("::").next() == Some(crate_prefix))
        .map(|(_, wrappers)| wrappers)
}

pub fn build_wrapper_coverage_map(
    pairs: &[(String, String)],
    complete_paths: &ElicitCompleteSet,
    wrapper_prereqs: &HashMap<String, TraitPrereqs>,
) -> WrapperCoverageMap {
    let mut map = WrapperCoverageMap::new();

    for (foreign, wrapper) in pairs {
        let coverage = WrapperCoverage {
            wrapper_path: wrapper.clone(),
            wrapper_elicit_complete: complete_paths.contains_path(wrapper),
            wrapper_prereqs: wrapper_prereqs
                .get(wrapper.as_str())
                .cloned()
                .unwrap_or_default(),
        };
        map.entry(foreign.clone()).or_default().push(coverage);
    }

    for providers in map.values_mut() {
        providers.sort_by(|left, right| left.wrapper_path.cmp(&right.wrapper_path));
    }

    map
}

pub fn join_wrapper_paths(wrappers: Option<&[WrapperCoverage]>) -> String {
    wrappers
        .unwrap_or(&[])
        .iter()
        .map(|wrapper| wrapper.wrapper_path.as_str())
        .collect::<Vec<_>>()
        .join(";")
}

fn merge_wrapper_prereqs(wrappers: Option<&[WrapperCoverage]>) -> TraitPrereqs {
    let mut merged = TraitPrereqs::default();
    for wrapper in wrappers.unwrap_or(&[]) {
        merged.merge(&wrapper.wrapper_prereqs);
    }
    merged
}

pub fn effective_missing_our_traits(
    direct_missing: &[&'static str],
    wrappers: Option<&[WrapperCoverage]>,
) -> Vec<&'static str> {
    let merged = merge_wrapper_prereqs(wrappers);
    direct_missing
        .iter()
        .copied()
        .filter(|trait_name| match *trait_name {
            "Elicitation" => !merged.elicitation_trait,
            "ElicitIntrospect" => !merged.elicit_introspect,
            "ElicitSpec" => !merged.elicit_spec,
            "ElicitPromptTree" => !merged.elicit_prompt_tree,
            "ToCodeLiteral" => !merged.to_code_literal,
            _ => true,
        })
        .collect()
}

pub fn covered_indirectly(wrappers: Option<&[WrapperCoverage]>) -> bool {
    wrappers.is_some_and(|known| {
        known.iter().any(|wrapper| {
            wrapper.wrapper_elicit_complete || wrapper.wrapper_prereqs.our_traits_complete()
        })
    })
}

pub fn indirect_elicit_complete(wrappers: Option<&[WrapperCoverage]>) -> bool {
    wrappers
        .unwrap_or(&[])
        .iter()
        .any(|wrapper| wrapper.wrapper_elicit_complete)
}

pub fn coverage_provider_label(
    direct_our_traits_complete: bool,
    indirect_elicit_complete: bool,
    effective_our_traits_complete: bool,
    covered_indirectly: bool,
) -> String {
    if direct_our_traits_complete {
        "direct".to_string()
    } else if indirect_elicit_complete && effective_our_traits_complete {
        "hybrid".to_string()
    } else if indirect_elicit_complete || covered_indirectly {
        "wrapper".to_string()
    } else {
        String::new()
    }
}
