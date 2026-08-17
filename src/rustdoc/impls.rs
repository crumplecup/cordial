use rustdoc_types::{ItemEnum, Type};

use super::inventory::{RustdocInventory, canonical_to_public_map};

/// One `impl Trait for Type` edge from rustdoc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitImplRecord {
    pub type_path: String,
    pub trait_path: String,
    pub trait_short: String,
}

/// Collect trait impl edges for types in `inventory`.
pub fn collect_trait_impls(inventory: &RustdocInventory) -> Vec<TraitImplRecord> {
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

    let mut records = Vec::new();
    for item in inventory.krate.index.values() {
        let ItemEnum::Impl(impl_item) = &item.inner else {
            continue;
        };
        let Some(trait_) = &impl_item.trait_ else {
            continue;
        };
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
        let trait_short = trait_.path.rsplit("::").next().unwrap_or("").to_string();
        records.push(TraitImplRecord {
            type_path,
            trait_path: trait_.path.clone(),
            trait_short,
        });
    }

    records.sort_by(|a, b| {
        a.type_path
            .cmp(&b.type_path)
            .then(a.trait_path.cmp(&b.trait_path))
    });
    records
}

/// Resolve the canonical path for an impl target type.
pub fn impl_target_path(krate: &rustdoc_types::Crate, ty: &Type) -> Option<String> {
    let Type::ResolvedPath(path) = ty else {
        return None;
    };
    krate
        .paths
        .get(&path.id)
        .map(|summary| summary.path.join("::"))
}
