//! `ElicitComplete` impl paths extracted from hub rustdoc JSON.

use std::collections::HashSet;
use std::path::Path;

use rustdoc_types::{GenericParamDefKind, ItemEnum, Type};

use crate::error::CordialResult;

use super::ELICIT_COMPLETE_TRAIT;
use super::inventory::RustdocInventory;

use tracing::instrument;
/// Types with `impl ElicitComplete for T` in a hub crate rustdoc snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ElicitCompleteSet {
    pub concrete: HashSet<String>,
    pub factory: HashSet<String>,
}

impl ElicitCompleteSet {
    #[instrument(level = "trace", skip(self, path))]
    pub fn contains_path(&self, path: &str) -> bool {
        self.concrete.contains(path) || self.factory.contains(path)
    }
}

/// Scan rustdoc JSON on disk for concrete and factory `ElicitComplete` impls.
#[instrument(level = "debug", err(level = "warn"))]
pub fn collect_elicit_complete_paths(
    json_path: &Path,
    local_crate_name: &str,
) -> CordialResult<ElicitCompleteSet> {
    let inventory = super::parse_rustdoc_json(json_path, local_crate_name)?;
    Ok(collect_elicit_complete_from_inventory(&inventory))
}

#[instrument(level = "debug", skip(inventory))]
pub fn collect_elicit_complete_from_inventory(inventory: &RustdocInventory) -> ElicitCompleteSet {
    let local_crate_name = inventory.crate_name.replace('-', "_");
    let mut concrete = HashSet::new();
    let mut factory = HashSet::new();

    for item in inventory.krate.index.values() {
        let ItemEnum::Impl(impl_item) = &item.inner else {
            continue;
        };
        let is_elicit_complete = impl_item
            .trait_
            .as_ref()
            .is_some_and(|t| t.path.rsplit("::").next() == Some(ELICIT_COMPLETE_TRAIT));
        if !is_elicit_complete {
            continue;
        }

        let is_factory = impl_item
            .generics
            .params
            .iter()
            .any(|param| matches!(param.kind, GenericParamDefKind::Type { .. }));

        let path = match &impl_item.for_ {
            Type::ResolvedPath(path) => inventory
                .krate
                .paths
                .get(&path.id)
                .map(|summary| summary.path.join("::"))
                .unwrap_or_else(|| {
                    path.path
                        .replace("crate::", &format!("{local_crate_name}::"))
                }),
            Type::Primitive(name) => name.clone(),
            _ => continue,
        };

        if is_factory {
            factory.insert(path);
        } else {
            concrete.insert(path);
        }
    }

    ElicitCompleteSet { concrete, factory }
}
