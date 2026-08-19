//! Trait prerequisite tracking for `ElicitComplete` coverage.

use std::collections::HashMap;

use rustdoc_types::{ItemEnum, Type};

use super::inventory::{RustdocInventory, canonical_to_public_map};

use tracing::instrument;
/// The eight supertraits required before `impl ElicitComplete`.
pub const ELICIT_COMPLETE_SUPERTRAITS: &[&str] = &[
    "Serialize",
    "Deserialize",
    "JsonSchema",
    "Elicitation",
    "ElicitIntrospect",
    "ElicitSpec",
    "ElicitPromptTree",
    "ToCodeLiteral",
];

pub const ELICIT_COMPLETE_TRAIT: &str = "ElicitComplete";

/// Which of the eight `ElicitComplete` supertraits a type already implements.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TraitPrereqs {
    pub serialize: bool,
    pub deserialize: bool,
    pub json_schema: bool,
    pub elicitation_trait: bool,
    pub elicit_introspect: bool,
    pub elicit_spec: bool,
    pub elicit_prompt_tree: bool,
    pub to_code_literal: bool,
    pub elicit_complete: bool,
}

impl TraitPrereqs {
    #[instrument(level = "trace", skip(self))]
    pub fn can_be_direct(&self) -> bool {
        self.serialize && self.deserialize && self.json_schema
    }

    #[instrument(level = "debug", skip(self))]
    pub fn missing_our_traits(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.elicitation_trait {
            missing.push("Elicitation");
        }
        if !self.elicit_introspect {
            missing.push("ElicitIntrospect");
        }
        if !self.elicit_spec {
            missing.push("ElicitSpec");
        }
        if !self.elicit_prompt_tree {
            missing.push("ElicitPromptTree");
        }
        if !self.to_code_literal {
            missing.push("ToCodeLiteral");
        }
        missing
    }

    #[instrument(level = "trace", skip(self))]
    pub fn our_traits_complete(&self) -> bool {
        self.missing_our_traits().is_empty()
    }

    #[instrument(level = "debug", skip(self))]
    pub fn external_blockers(&self) -> Vec<&'static str> {
        let mut blockers = Vec::new();
        if !self.serialize {
            blockers.push("Serialize");
        }
        if !self.deserialize {
            blockers.push("Deserialize");
        }
        if !self.json_schema {
            blockers.push("JsonSchema");
        }
        blockers
    }

    #[instrument(level = "debug", ret)]
    pub fn from_trait_short(trait_short: &str) -> Self {
        let mut prereqs = Self::default();
        prereqs.apply_trait_short(trait_short);
        prereqs
    }

    #[instrument(level = "debug", skip(self))]
    pub fn apply_trait_short(&mut self, trait_short: &str) {
        match trait_short {
            "Serialize" => self.serialize = true,
            "Deserialize" => self.deserialize = true,
            "JsonSchema" => self.json_schema = true,
            "Elicitation" => self.elicitation_trait = true,
            "ElicitIntrospect" => self.elicit_introspect = true,
            "ElicitSpec" => self.elicit_spec = true,
            "ElicitPromptTree" => self.elicit_prompt_tree = true,
            "ToCodeLiteral" => self.to_code_literal = true,
            "ElicitComplete" => self.elicit_complete = true,
            _ => {}
        }
    }

    #[instrument(level = "debug", skip(self, other))]
    pub fn merge(&mut self, other: &Self) {
        self.serialize |= other.serialize;
        self.deserialize |= other.deserialize;
        self.json_schema |= other.json_schema;
        self.elicitation_trait |= other.elicitation_trait;
        self.elicit_introspect |= other.elicit_introspect;
        self.elicit_spec |= other.elicit_spec;
        self.elicit_prompt_tree |= other.elicit_prompt_tree;
        self.to_code_literal |= other.to_code_literal;
        self.elicit_complete |= other.elicit_complete;
    }
}

/// Scan rustdoc JSON for trait prereqs on inventory type paths.
#[instrument(level = "debug", skip(inventory))]
pub fn collect_trait_prereqs_for_inventory(
    inventory: &RustdocInventory,
) -> HashMap<String, TraitPrereqs> {
    let tracked: std::collections::HashSet<String> = inventory
        .type_items()
        .map(|item| item.path.clone())
        .collect();
    let canonical_map = canonical_to_public_map(inventory);
    let extended: std::collections::HashSet<String> = tracked
        .iter()
        .cloned()
        .chain(canonical_map.keys().cloned())
        .collect();

    let mut map: HashMap<String, TraitPrereqs> = HashMap::new();
    for item in inventory.krate.index.values() {
        let ItemEnum::Impl(impl_item) = &item.inner else {
            continue;
        };
        let Some(trait_) = &impl_item.trait_ else {
            continue;
        };
        let trait_short = trait_.path.rsplit("::").next().unwrap_or("");
        if !ELICIT_COMPLETE_SUPERTRAITS.contains(&trait_short)
            && trait_short != ELICIT_COMPLETE_TRAIT
        {
            continue;
        }
        let Type::ResolvedPath(type_path) = &impl_item.for_ else {
            continue;
        };
        let Some(summary) = inventory.krate.paths.get(&type_path.id) else {
            continue;
        };
        let canonical = summary.path.join("::");
        if !extended.contains(&canonical) {
            continue;
        }
        let type_path = canonical_map.get(&canonical).cloned().unwrap_or(canonical);
        map.entry(type_path)
            .or_default()
            .apply_trait_short(trait_short);
    }
    map
}

/// Merge prereqs from [`EdgeKind::Implements`] trait short names.
#[instrument(level = "debug")]
pub fn prereqs_from_trait_shorts(trait_shorts: &[String]) -> TraitPrereqs {
    let mut prereqs = TraitPrereqs::default();
    for short in trait_shorts {
        prereqs.apply_trait_short(short);
    }
    prereqs
}
