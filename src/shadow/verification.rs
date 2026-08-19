//! Shadow row verification fields from hub/shadow trait prereqs.

use std::collections::HashMap;

use crate::rustdoc::{ElicitCompleteSet, RustdocItem, TraitPrereqs};

use super::types::{ShadowRow, ShadowStatus};

use tracing::instrument;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowImplStatus {
    Complete,
    CompleteFactory,
    Missing,
}

impl ShadowImplStatus {
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "Complete",
            Self::CompleteFactory => "CompleteFactory",
            Self::Missing => "Missing",
        }
    }
}

#[instrument(level = "debug", skip(item, complete))]
pub fn shadow_impl_status(item: &RustdocItem, complete: &ElicitCompleteSet) -> ShadowImplStatus {
    if !item.kind.is_type() {
        return ShadowImplStatus::Missing;
    }
    if complete.contains_path(&item.path) {
        if complete.factory.contains(&item.path) {
            ShadowImplStatus::CompleteFactory
        } else {
            ShadowImplStatus::Complete
        }
    } else {
        ShadowImplStatus::Missing
    }
}

#[instrument(level = "debug", skip(item, prereqs))]
pub fn shadow_can_be_direct(item: &RustdocItem, prereqs: &HashMap<String, TraitPrereqs>) -> String {
    if !item.kind.is_type() {
        return String::new();
    }
    prereqs
        .get(&item.path)
        .map(|p| p.can_be_direct().to_string())
        .unwrap_or_else(|| "false".to_string())
}

#[instrument(level = "debug", skip(item, prereqs))]
pub fn shadow_missing_external_traits(
    item: &RustdocItem,
    prereqs: &HashMap<String, TraitPrereqs>,
) -> String {
    if !item.kind.is_type() {
        return String::new();
    }
    prereqs
        .get(&item.path)
        .map(|p| external_blockers_absent(p).join(";"))
        .unwrap_or_else(|| "Serialize(absent);Deserialize(absent);JsonSchema(absent)".to_string())
}

#[instrument(level = "debug", skip(item, prereqs))]
pub fn shadow_missing_our_traits(
    item: &RustdocItem,
    prereqs: &HashMap<String, TraitPrereqs>,
) -> String {
    if !item.kind.is_type() {
        return String::new();
    }
    prereqs
        .get(&item.path)
        .map(|p| p.missing_our_traits().join(";"))
        .unwrap_or_else(|| {
            [
                "Elicitation",
                "ElicitIntrospect",
                "ElicitSpec",
                "ElicitPromptTree",
                "ToCodeLiteral",
            ]
            .join(";")
        })
}

#[instrument(level = "debug", skip(row))]
pub fn shadow_verification_gap(row: &ShadowRow) -> bool {
    if !matches!(row.status, ShadowStatus::Covered | ShadowStatus::Drifted)
        || !row.item_kind.is_type()
    {
        return false;
    }
    row.shadow_elicit_impl != ShadowImplStatus::Complete.as_str()
        && row.shadow_elicit_impl != ShadowImplStatus::CompleteFactory.as_str()
}

#[instrument(level = "debug", skip(prereqs))]
fn external_blockers_absent(prereqs: &TraitPrereqs) -> Vec<String> {
    let mut blockers = Vec::new();
    if !prereqs.serialize {
        blockers.push("Serialize(absent)".to_string());
    }
    if !prereqs.deserialize {
        blockers.push("Deserialize(absent)".to_string());
    }
    if !prereqs.json_schema {
        blockers.push("JsonSchema(absent)".to_string());
    }
    blockers
}
