//! Public rustdoc inventory extraction (ported from elicit_doc `collect/inventory`).

use std::collections::{HashMap, HashSet};

use rustdoc_types::Crate;
use tracing::{debug, instrument};

use super::InventoryItemKind;
use crate::rustdoc::stability::item_attrs_are_unstable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedItem {
    pub path: Vec<String>,
    pub kind: InventoryItemKind,
    pub name: String,
    pub is_generic: bool,
    pub alias_target: Option<String>,
    pub is_unstable: bool,
}

impl ExtractedItem {
    pub fn path_str(&self) -> String {
        self.path.join("::")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtractedItemKind {
    Struct,
    Enum,
    Trait,
    TypeAlias,
    Function,
    Macro,
    Constant,
    Module,
    Other,
}

impl ExtractedItemKind {
    fn to_inventory(self) -> InventoryItemKind {
        match self {
            Self::Struct => InventoryItemKind::Struct,
            Self::Enum => InventoryItemKind::Enum,
            Self::Trait => InventoryItemKind::Trait,
            Self::TypeAlias => InventoryItemKind::TypeAlias,
            Self::Function => InventoryItemKind::Function,
            _ => InventoryItemKind::Other,
        }
    }
}

pub fn extract_public_items(
    krate: &Crate,
    own_crate: &str,
    prefix_match: bool,
) -> Vec<ExtractedItem> {
    extract_items(krate, own_crate, prefix_match)
}

/// Extract all public items from a rustdoc [`Crate`] into our flat [`ExtractedItem`] list.
///
/// For re-exporting umbrella crates (like `bevy`) the `index` only contains
/// a handful of module items while all re-exported items live in `paths`.
/// We therefore build the inventory from `paths` and look up the `index` entry
/// only for additional generics detail when available.
///
/// `prefix_match`: when `true`, items are accepted if their first path segment
/// **starts with** `own_crate` (e.g. `"bevy"` accepts `bevy_ecs::*`, `bevy_math::*`).
/// When `false`, the first segment must equal `own_crate` exactly.
#[instrument(skip(krate), fields(own_crate, prefix_match))]
#[doc(hidden)]
pub fn extract_items(
    krate: &rustdoc_types::Crate,
    own_crate: &str,
    prefix_match: bool,
) -> Vec<ExtractedItem> {
    let mut items = Vec::new();
    let mut seen_paths = HashSet::new();
    // Rustdoc JSON paths always use underscores even when the Cargo.toml package
    // name is hyphenated (e.g. "geo-types" → "geo_types").
    let own_crate_normalized = own_crate.replace('-', "_");
    let own_crate_key = own_crate_normalized.as_str();
    let public_reexport_aliases =
        collect_public_same_crate_reexport_aliases(krate, own_crate_key, prefix_match);
    let public_module_paths = collect_public_module_paths(krate, own_crate_key, prefix_match);

    for item in public_reexport_aliases.values() {
        seen_paths.insert(item.path_str());
        items.push(item.clone());
    }

    for (id, summary) in &krate.paths {
        if !path_matches_scope(&summary.path, own_crate_key, prefix_match) {
            continue;
        }
        if public_reexport_aliases.contains_key(id) {
            debug!(
                target_path = %summary.path.join("::"),
                "skipping canonical same-crate path in favor of public reexport alias"
            );
            continue;
        }

        let Some(item) = build_inventory_item(krate, id, summary) else {
            continue;
        };
        if !item_path_is_publicly_reachable(&item, &public_module_paths) {
            debug!(
                item_path = %item.path_str(),
                "skipping non-publicly-reachable canonical path"
            );
            continue;
        }
        seen_paths.insert(item.path_str());
        items.push(item);
    }

    for item in
        collect_public_reexport_dependency_items(krate, own_crate_key, prefix_match, &seen_paths)
    {
        if seen_paths.insert(item.path_str()) {
            items.push(item);
        }
    }

    for item in
        collect_public_signature_dependency_items(krate, own_crate_key, prefix_match, &seen_paths)
    {
        if seen_paths.insert(item.path_str()) {
            items.push(item);
        }
    }

    items.sort_by(|a, b| a.path.cmp(&b.path));
    tracing::debug!(count = items.len(), "extracted items");
    items
}

/// Build a map from every item ID to the path of its direct parent module.
///
/// Rustdoc JSON `Use` items for `pub use` re-exports are often absent from
/// `krate.paths`.  Walking the module tree gives us an alternative way to
/// infer the re-export path: `parent_module_path + use_item.name`.
#[instrument(skip(krate))]
fn build_parent_module_paths(
    krate: &rustdoc_types::Crate,
) -> HashMap<rustdoc_types::Id, Vec<String>> {
    let mut parent_paths: HashMap<rustdoc_types::Id, Vec<String>> = HashMap::new();
    for (mod_id, item) in &krate.index {
        let rustdoc_types::ItemEnum::Module(module) = &item.inner else {
            continue;
        };
        let Some(summary) = krate.paths.get(mod_id) else {
            continue;
        };
        for child_id in &module.items {
            parent_paths.insert(*child_id, summary.path.clone());
        }
    }
    parent_paths
}

#[instrument(skip(krate), fields(own_crate_key, prefix_match))]
pub(super) fn collect_public_same_crate_reexport_aliases(
    krate: &rustdoc_types::Crate,
    own_crate_key: &str,
    prefix_match: bool,
) -> HashMap<rustdoc_types::Id, ExtractedItem> {
    let parent_module_paths = build_parent_module_paths(krate);
    let mut aliases: HashMap<rustdoc_types::Id, ExtractedItem> = HashMap::new();

    for (id, item) in &krate.index {
        let rustdoc_types::ItemEnum::Use(use_item) = &item.inner else {
            continue;
        };
        if !item_is_public(item) {
            continue;
        }

        // Prefer the path recorded in krate.paths; fall back to inferring from
        // the parent module when the Use item itself has no paths entry (common
        // for `pub use` re-exports in crates like chrono that restructure their
        // public API through private intermediate modules).
        let use_path: Vec<String> = if let Some(summary) = krate.paths.get(id) {
            summary.path.clone()
        } else {
            let Some(parent_path) = parent_module_paths.get(id) else {
                continue;
            };
            if !path_matches_scope(parent_path, own_crate_key, prefix_match) {
                continue;
            }
            let mut path = parent_path.clone();
            path.push(use_item.name.clone());
            path
        };

        if !path_matches_scope(&use_path, own_crate_key, prefix_match) {
            continue;
        }

        let Some(target_id) = &use_item.id else {
            continue;
        };
        let Some(target_summary) = krate.paths.get(target_id) else {
            continue;
        };
        if !path_matches_scope(&target_summary.path, own_crate_key, prefix_match) {
            continue;
        }

        let Some(alias_item) =
            build_inventory_item_with_path(krate, target_id, target_summary.kind, use_path)
        else {
            continue;
        };

        match aliases.entry(*target_id) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                debug!(
                    target_path = %target_summary.path.join("::"),
                    alias_path = %alias_item.path_str(),
                    "recorded same-crate public reexport alias"
                );
                slot.insert(alias_item);
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                if item_path_preferred_over(&alias_item.path, &slot.get().path) {
                    debug!(
                        target_path = %target_summary.path.join("::"),
                        previous_alias = %slot.get().path_str(),
                        alias_path = %alias_item.path_str(),
                        "replaced same-crate public reexport alias with shorter public path"
                    );
                    slot.insert(alias_item);
                }
            }
        }
    }

    aliases
}

#[instrument(skip(krate, existing_paths), fields(own_crate_key, prefix_match, existing_count = existing_paths.len()))]
fn collect_public_reexport_dependency_items(
    krate: &rustdoc_types::Crate,
    own_crate_key: &str,
    prefix_match: bool,
    existing_paths: &HashSet<String>,
) -> Vec<ExtractedItem> {
    let mut discovered = Vec::new();
    let mut seen = existing_paths.clone();

    for (id, item) in &krate.index {
        let rustdoc_types::ItemEnum::Use(use_item) = &item.inner else {
            continue;
        };
        if !item_is_public(item) {
            continue;
        }
        let Some(use_summary) = krate.paths.get(id) else {
            continue;
        };
        if !path_matches_scope(&use_summary.path, own_crate_key, prefix_match) {
            continue;
        }
        let Some(target_id) = &use_item.id else {
            continue;
        };
        let Some(target_summary) = krate.paths.get(target_id) else {
            continue;
        };

        let target_path = target_summary.path.join("::");
        if path_matches_scope(&target_summary.path, own_crate_key, prefix_match)
            || target_path.starts_with("std::")
            || target_path.starts_with("core::")
            || target_path.starts_with("alloc::")
        {
            continue;
        }

        if seen.insert(target_path)
            && let Some(item) = build_inventory_item(krate, target_id, target_summary)
        {
            discovered.push(item);
        }
    }

    discovered
}

#[instrument(fields(own_crate_key, prefix_match))]
fn path_matches_scope(path: &[String], own_crate_key: &str, prefix_match: bool) -> bool {
    path.first()
        .map(|segment| {
            if prefix_match {
                segment.starts_with(own_crate_key)
            } else {
                segment == own_crate_key
            }
        })
        .unwrap_or(false)
}

#[instrument(skip(krate, summary), fields(id = ?id))]
fn build_inventory_item(
    krate: &rustdoc_types::Crate,
    id: &rustdoc_types::Id,
    summary: &rustdoc_types::ItemSummary,
) -> Option<ExtractedItem> {
    build_inventory_item_with_path(krate, id, summary.kind, summary.path.clone())
}

#[instrument(skip(krate), fields(id = ?id, kind = ?kind))]
fn build_inventory_item_with_path(
    krate: &rustdoc_types::Crate,
    id: &rustdoc_types::Id,
    kind: rustdoc_types::ItemKind,
    path: Vec<String>,
) -> Option<ExtractedItem> {
    let kind = match kind {
        rustdoc_types::ItemKind::Struct => ExtractedItemKind::Struct,
        rustdoc_types::ItemKind::Enum => ExtractedItemKind::Enum,
        rustdoc_types::ItemKind::Trait => ExtractedItemKind::Trait,
        rustdoc_types::ItemKind::TypeAlias => ExtractedItemKind::TypeAlias,
        rustdoc_types::ItemKind::Function => ExtractedItemKind::Function,
        rustdoc_types::ItemKind::Primitive => ExtractedItemKind::Other,
        rustdoc_types::ItemKind::Module
        | rustdoc_types::ItemKind::Macro
        | rustdoc_types::ItemKind::Constant => return None,
        _ => return None,
    };

    let name = path.last().cloned().unwrap_or_default();
    if name.is_empty() {
        return None;
    }

    let index_item = krate.index.get(id);

    let (is_generic, _lifetime_params, _type_params) = index_item
        .map(|item| {
            let (_, g, lp, tp) = classify_item(item);
            (g, lp, tp)
        })
        .unwrap_or((false, vec![], vec![]));

    // For type aliases, extract the path of the aliased type so the coverage
    // checker can fall back to the underlying type's impl status.
    let alias_target = if kind == ExtractedItemKind::TypeAlias {
        index_item.and_then(|item| {
            if let rustdoc_types::ItemEnum::TypeAlias(ta) = &item.inner {
                alias_target_path(&ta.type_)
            } else {
                None
            }
        })
    } else {
        None
    };

    Some(ExtractedItem {
        path,
        kind: kind.to_inventory(),
        name,
        is_generic,
        alias_target,
        is_unstable: index_item.is_some_and(|item| rustdoc_item_is_unstable(krate, id, item)),
    })
}

/// Whether a rustdoc item is nightly/unstable (not usable on stable without a feature gate).
///
/// Checks the item's own `attrs` first. Some items' instability is declared
/// only on an *enclosing module*, never repeated per item —
/// `core::str::pattern::*` (gated on `mod pattern` itself) and
/// `core::intrinsics::mir::*` are two confirmed real examples. When the
/// item's own attrs carry no marker, [`walk_stability_ancestors`] climbs
/// its enclosing modules and checks each of those.
///
/// A harder case needs more than plain containment: `core::simd`'s 121 SIMD
/// vector aliases (`f32x1`, `Mask`, …) are *defined* in an unrelated
/// private module with no stability marker anywhere in its own containment
/// ancestry, and reach the gated `core::simd` module through *two* levels
/// of `pub use` re-export — first a named re-export into an internal
/// prelude module, then a glob re-export (`pub use ...::*`) from that
/// internal module's parent into the public, gated one. A single
/// containment-then-one-reexport-hop check (an earlier version of this
/// function) can't see a two-hop chain like that; the walk below treats
/// "is the target of a `Use` item" as just another edge to climb, at every
/// step, so any number of interleaved containment and re-export hops
/// resolves the same way.
#[instrument(skip(krate, item), fields(id = id.0))]
fn rustdoc_item_is_unstable(
    krate: &rustdoc_types::Crate,
    id: &rustdoc_types::Id,
    item: &rustdoc_types::Item,
) -> bool {
    if item_attrs_are_unstable(item) {
        return true;
    }

    let ancestry = module_ancestry(krate);
    let reexports = reexporting_use_ids(krate);
    walk_stability_ancestors(krate, &ancestry, &reexports, *id)
}

/// Breadth-first search over the union of two edge types rooted at `start`,
/// returning whether any reached node's own `attrs` carry an unstable
/// marker: containment (`ancestry`: item -> its direct enclosing module)
/// and re-export (`reexports`: item -> `Use` items that re-export it, each
/// of which is itself subject to the same two edge types). Depth-capped and
/// deduplicated against a cycle in malformed rustdoc JSON or an unexpected
/// re-export loop.
#[instrument(skip(krate, ancestry, reexports))]
fn walk_stability_ancestors(
    krate: &rustdoc_types::Crate,
    ancestry: &ModuleAncestryMap,
    reexports: &ReexportingUseMap,
    start: rustdoc_types::Id,
) -> bool {
    let mut visited = HashSet::from([start]);
    let mut frontier = vec![start];

    for depth in 0..32 {
        if frontier.is_empty() {
            tracing::trace!(
                start = start.0,
                depth,
                "stability ancestor walk: frontier exhausted, stopping"
            );
            return false;
        }

        let mut next_frontier = Vec::new();
        for current in &frontier {
            let mut candidates = Vec::new();
            if let Some(parent_id) = ancestry.get(current) {
                candidates.push(*parent_id);
            }
            if let Some(use_ids) = reexports.get(current) {
                candidates.extend(use_ids.iter().copied());
            }

            for candidate_id in candidates {
                if !visited.insert(candidate_id) {
                    continue;
                }
                let Some(candidate_item) = krate.index.get(&candidate_id) else {
                    continue;
                };
                if item_attrs_are_unstable(candidate_item) {
                    tracing::debug!(
                        start = start.0,
                        depth,
                        unstable_ancestor_id = candidate_id.0,
                        "stability ancestor walk: found unstable ancestor"
                    );
                    return true;
                }
                next_frontier.push(candidate_id);
            }
        }
        frontier = next_frontier;
    }
    tracing::trace!(
        start = start.0,
        "stability ancestor walk: exhausted depth cap"
    );
    false
}

type ModuleAncestryMap = HashMap<rustdoc_types::Id, rustdoc_types::Id>;

/// Map every item ID to the ID of its direct enclosing module, for `krate`.
///
/// Recomputed fresh on every call rather than cached: an earlier version of
/// this function cached the result keyed by `krate`'s address
/// (`*const Crate`), which is unsound here — `parse_rustdoc_json` calls
/// `extract_items` once per crate (std, then core, then alloc) with an
/// owned local `krate`, and a later call's local can legitimately reuse the
/// exact stack address an earlier call's local occupied. That address reuse
/// caused `alloc`'s ancestry lookups to silently reuse `std`'s ancestry map
/// — `Id`s are only unique *within* one rustdoc JSON document, so the same
/// numeric id means a completely different item in a different crate's
/// document, and the walk produced real false positives (`alloc::string::String`
/// and `alloc::borrow::Cow` both got flagged unstable via a bogus ancestor).
/// `krate.index` is only tens of thousands of entries even for `core`, and
/// this only runs for items whose own `attrs` already showed no stability
/// marker (a minority), so recomputing here costs nothing that matters next
/// to the rustdoc build this whole pipeline already pays for.
#[instrument(skip(krate))]
fn module_ancestry(krate: &rustdoc_types::Crate) -> ModuleAncestryMap {
    let mut ancestry = HashMap::new();
    let mut module_count = 0usize;
    for (mod_id, item) in &krate.index {
        let rustdoc_types::ItemEnum::Module(module) = &item.inner else {
            continue;
        };
        module_count += 1;
        for child_id in &module.items {
            ancestry.insert(*child_id, *mod_id);
        }
    }
    tracing::debug!(
        module_count,
        child_count = ancestry.len(),
        "built module ancestry map"
    );
    ancestry
}

type ReexportingUseMap = HashMap<rustdoc_types::Id, Vec<rustdoc_types::Id>>;

/// Map every item ID to the `Use` items (by their own id) that re-export it.
///
/// The inverse of `Use::id`: rustdoc records each `pub use` as its own
/// `ItemEnum::Use` item carrying the *target*'s id, not the other way
/// around, so finding "what re-exports this item" needs this reverse index.
/// Like [`module_ancestry`], recomputed fresh per call rather than cached —
/// same reasoning (see that function's doc comment), and this only runs as
/// a fallback for the minority of items whose direct containment ancestry
/// already came up empty.
#[instrument(skip(krate))]
fn reexporting_use_ids(krate: &rustdoc_types::Crate) -> ReexportingUseMap {
    let mut reexports: ReexportingUseMap = HashMap::new();
    let mut use_item_count = 0usize;
    for (use_id, item) in &krate.index {
        let rustdoc_types::ItemEnum::Use(use_item) = &item.inner else {
            continue;
        };
        let Some(target_id) = use_item.id else {
            continue;
        };
        use_item_count += 1;
        reexports.entry(target_id).or_default().push(*use_id);
    }
    tracing::debug!(
        use_item_count,
        target_count = reexports.len(),
        "built reexporting-use-id map"
    );
    reexports
}

/// Extract a human-readable path string from a rustdoc `Type`, used to record
/// what a type alias resolves to. Handles `ResolvedPath` (another named
/// item), `Primitive` (`dev_t -> u64`), and `RawPointer` (`HANDLE -> *mut
/// c_void`, recursing into the pointee so the result is still shape-matchable
/// by `type_has_trait_impl`'s `compound_primitive_shape` fallback even though
/// the pointee itself is a `ResolvedPath`, not a bare primitive). Returns
/// `None` for function pointers, tuples, and other complex types.
#[instrument(skip(ty))]
fn alias_target_path(ty: &rustdoc_types::Type) -> Option<String> {
    match ty {
        rustdoc_types::Type::ResolvedPath(p) => Some(p.path.clone()),
        rustdoc_types::Type::Primitive(name) => Some(name.clone()),
        rustdoc_types::Type::RawPointer { is_mutable, type_ } => {
            let pointee = alias_target_path(type_).unwrap_or_else(|| "_".to_string());
            let qualifier = if *is_mutable { "mut" } else { "const" };
            Some(format!("*{qualifier} {pointee}"))
        }
        _ => None,
    }
}

#[instrument(skip(krate), fields(own_crate_key, prefix_match))]
fn collect_public_module_paths(
    krate: &rustdoc_types::Crate,
    own_crate_key: &str,
    prefix_match: bool,
) -> HashSet<String> {
    krate
        .paths
        .iter()
        .filter_map(|(id, summary)| {
            if !path_matches_scope(&summary.path, own_crate_key, prefix_match)
                || summary.kind != rustdoc_types::ItemKind::Module
            {
                return None;
            }
            let item = krate.index.get(id)?;
            item_is_public(item).then_some(summary.path.join("::"))
        })
        .collect()
}

#[instrument(skip(item, public_module_paths))]
fn item_path_is_publicly_reachable(
    item: &ExtractedItem,
    public_module_paths: &HashSet<String>,
) -> bool {
    if item.path.len() <= 2 {
        return true;
    }

    for idx in 1..item.path.len() - 1 {
        let module_path = item.path[..=idx].join("::");
        if !public_module_paths.contains(&module_path) {
            debug!(
                item_path = %item.path_str(),
                missing_public_module = %module_path,
                "canonical path is not publicly reachable"
            );
            return false;
        }
    }

    true
}

#[instrument(skip(candidate, incumbent))]
fn item_path_preferred_over(candidate: &[String], incumbent: &[String]) -> bool {
    candidate.len() < incumbent.len()
        || (candidate.len() == incumbent.len() && candidate < incumbent)
}

#[instrument(
    skip(krate, existing_paths),
    fields(own_crate_key, prefix_match, existing_count = existing_paths.len())
)]
fn collect_public_signature_dependency_items(
    krate: &rustdoc_types::Crate,
    own_crate_key: &str,
    prefix_match: bool,
    existing_paths: &HashSet<String>,
) -> Vec<ExtractedItem> {
    let mut discovered = Vec::new();
    let mut seen = existing_paths.clone();

    for (id, item) in &krate.index {
        match &item.inner {
            rustdoc_types::ItemEnum::Function(function)
                if item_is_public(item)
                    && krate.paths.get(id).is_some_and(|summary| {
                        path_matches_scope(&summary.path, own_crate_key, prefix_match)
                    }) =>
            {
                collect_items_from_function_signature(
                    krate,
                    function,
                    own_crate_key,
                    prefix_match,
                    &mut seen,
                    &mut discovered,
                );
            }
            rustdoc_types::ItemEnum::Trait(trait_item)
                if item_is_public(item)
                    && krate.paths.get(id).is_some_and(|summary| {
                        path_matches_scope(&summary.path, own_crate_key, prefix_match)
                    }) =>
            {
                for child_id in &trait_item.items {
                    let Some(child) = krate.index.get(child_id) else {
                        continue;
                    };
                    if !item_is_public(child) {
                        continue;
                    }
                    if let rustdoc_types::ItemEnum::Function(function) = &child.inner {
                        collect_items_from_function_signature(
                            krate,
                            function,
                            own_crate_key,
                            prefix_match,
                            &mut seen,
                            &mut discovered,
                        );
                    }
                }
            }
            rustdoc_types::ItemEnum::Impl(impl_item)
                if impl_item.trait_.is_none()
                    && inherent_impl_targets_scope(
                        krate,
                        impl_item,
                        own_crate_key,
                        prefix_match,
                    ) =>
            {
                for child_id in &impl_item.items {
                    let Some(child) = krate.index.get(child_id) else {
                        continue;
                    };
                    if !item_is_public(child) {
                        continue;
                    }
                    if let rustdoc_types::ItemEnum::Function(function) = &child.inner {
                        collect_items_from_function_signature(
                            krate,
                            function,
                            own_crate_key,
                            prefix_match,
                            &mut seen,
                            &mut discovered,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    debug!(
        discovered_count = discovered.len(),
        "collected public signature dependency items"
    );

    discovered
}

#[instrument(skip(item))]
pub(super) fn item_is_public(item: &rustdoc_types::Item) -> bool {
    matches!(item.visibility, rustdoc_types::Visibility::Public)
}

#[instrument(skip(krate, impl_item), fields(own_crate_key, prefix_match))]
fn inherent_impl_targets_scope(
    krate: &rustdoc_types::Crate,
    impl_item: &rustdoc_types::Impl,
    own_crate_key: &str,
    prefix_match: bool,
) -> bool {
    let rustdoc_types::Type::ResolvedPath(resolved) = &impl_item.for_ else {
        return false;
    };
    krate
        .paths
        .get(&resolved.id)
        .is_some_and(|summary| path_matches_scope(&summary.path, own_crate_key, prefix_match))
}

#[instrument(
    skip(krate, function, seen, discovered),
    fields(input_count = function.sig.inputs.len(), has_output = function.sig.output.is_some())
)]
fn collect_items_from_function_signature(
    krate: &rustdoc_types::Crate,
    function: &rustdoc_types::Function,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    for (_, input) in &function.sig.inputs {
        collect_items_from_type(krate, input, own_crate_key, prefix_match, seen, discovered);
    }
    if let Some(output) = &function.sig.output {
        collect_items_from_type(krate, output, own_crate_key, prefix_match, seen, discovered);
    }
    collect_items_from_generics(
        krate,
        &function.generics,
        own_crate_key,
        prefix_match,
        seen,
        discovered,
    );

    debug!(
        discovered_count = discovered.len(),
        "processed function signature for dependency discovery"
    );
}

#[instrument(
    skip(krate, ty, seen, discovered),
    fields(type_kind = type_kind_name(ty))
)]
fn collect_items_from_type(
    krate: &rustdoc_types::Crate,
    ty: &rustdoc_types::Type,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    match ty {
        rustdoc_types::Type::ResolvedPath(resolved) => {
            if let Some(summary) = krate.paths.get(&resolved.id) {
                let path = summary.path.join("::");
                if !path_matches_scope(&summary.path, own_crate_key, prefix_match)
                    && !path.starts_with("std::")
                    && !path.starts_with("core::")
                    && !path.starts_with("alloc::")
                {
                    if seen.insert(path) {
                        debug!(
                            discovered_path = %summary.path.join("::"),
                            "discovered signature dependency from resolved type"
                        );
                        if let Some(item) = build_inventory_item(krate, &resolved.id, summary) {
                            discovered.push(item);
                        }
                    }
                } else {
                    debug!(
                        candidate_path = %summary.path.join("::"),
                        in_scope = path_matches_scope(&summary.path, own_crate_key, prefix_match),
                        std_like = path.starts_with("std::")
                            || path.starts_with("core::")
                            || path.starts_with("alloc::"),
                        "skipping resolved type during signature dependency discovery"
                    );
                }
            }
            if let Some(args) = &resolved.args {
                collect_items_from_generic_args(
                    krate,
                    args,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
        }
        rustdoc_types::Type::BorrowedRef { type_, .. }
        | rustdoc_types::Type::RawPointer { type_, .. }
        | rustdoc_types::Type::Slice(type_)
        | rustdoc_types::Type::Array { type_, .. } => {
            collect_items_from_type(krate, type_, own_crate_key, prefix_match, seen, discovered)
        }
        rustdoc_types::Type::Tuple(items) => {
            for item in items {
                collect_items_from_type(krate, item, own_crate_key, prefix_match, seen, discovered);
            }
        }
        rustdoc_types::Type::FunctionPointer(function_pointer) => {
            for (_, input) in &function_pointer.sig.inputs {
                collect_items_from_type(
                    krate,
                    input,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            if let Some(output) = &function_pointer.sig.output {
                collect_items_from_type(
                    krate,
                    output,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            collect_items_from_generic_param_defs(
                krate,
                &function_pointer.generic_params,
                own_crate_key,
                prefix_match,
                seen,
                discovered,
            );
        }
        rustdoc_types::Type::DynTrait(dyn_trait) => {
            for bound in &dyn_trait.traits {
                collect_items_from_poly_trait(
                    krate,
                    bound,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
        }
        rustdoc_types::Type::ImplTrait(bounds) => {
            for bound in bounds {
                collect_items_from_generic_bound(
                    krate,
                    bound,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
        }
        rustdoc_types::Type::QualifiedPath {
            self_type,
            trait_,
            args,
            ..
        } => {
            collect_items_from_type(
                krate,
                self_type,
                own_crate_key,
                prefix_match,
                seen,
                discovered,
            );
            if let Some(trait_) = trait_ {
                collect_items_from_path(
                    krate,
                    trait_,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            if let Some(args) = args {
                collect_items_from_generic_args(
                    krate,
                    args,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
        }
        rustdoc_types::Type::Primitive(_)
        | rustdoc_types::Type::Generic(_)
        | rustdoc_types::Type::Infer => {}
        _ => {}
    }
}

#[instrument(
    skip(krate, path, seen, discovered),
    fields(path = %path.path)
)]
fn collect_items_from_path(
    krate: &rustdoc_types::Crate,
    path: &rustdoc_types::Path,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    if let Some(summary) = krate.paths.get(&path.id) {
        let path_str = summary.path.join("::");
        if !path_matches_scope(&summary.path, own_crate_key, prefix_match)
            && !path_str.starts_with("std::")
            && !path_str.starts_with("core::")
            && !path_str.starts_with("alloc::")
        {
            if seen.insert(path_str) {
                debug!(
                    discovered_path = %summary.path.join("::"),
                    "discovered signature dependency from path"
                );
                if let Some(item) = build_inventory_item(krate, &path.id, summary) {
                    discovered.push(item);
                }
            }
        } else {
            debug!(
                candidate_path = %summary.path.join("::"),
                in_scope = path_matches_scope(&summary.path, own_crate_key, prefix_match),
                std_like = path_str.starts_with("std::")
                    || path_str.starts_with("core::")
                    || path_str.starts_with("alloc::"),
                "skipping path during signature dependency discovery"
            );
        }
    }
    if let Some(args) = &path.args {
        collect_items_from_generic_args(krate, args, own_crate_key, prefix_match, seen, discovered);
    }
}

#[instrument(skip(krate, args, own_crate_key, prefix_match, seen, discovered))]
fn collect_items_from_generic_args(
    krate: &rustdoc_types::Crate,
    args: &rustdoc_types::GenericArgs,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    match args {
        rustdoc_types::GenericArgs::AngleBracketed { args, constraints } => {
            for arg in args {
                if let rustdoc_types::GenericArg::Type(ty) = arg {
                    collect_items_from_type(
                        krate,
                        ty,
                        own_crate_key,
                        prefix_match,
                        seen,
                        discovered,
                    );
                }
            }
            for constraint in constraints {
                if let Some(args) = &constraint.args {
                    collect_items_from_generic_args(
                        krate,
                        args,
                        own_crate_key,
                        prefix_match,
                        seen,
                        discovered,
                    );
                }
                match &constraint.binding {
                    rustdoc_types::AssocItemConstraintKind::Equality(term) => {
                        collect_items_from_term(
                            krate,
                            term,
                            own_crate_key,
                            prefix_match,
                            seen,
                            discovered,
                        );
                    }
                    rustdoc_types::AssocItemConstraintKind::Constraint(bounds) => {
                        for bound in bounds {
                            collect_items_from_generic_bound(
                                krate,
                                bound,
                                own_crate_key,
                                prefix_match,
                                seen,
                                discovered,
                            );
                        }
                    }
                }
            }
        }
        rustdoc_types::GenericArgs::Parenthesized { inputs, output } => {
            for input in inputs {
                collect_items_from_type(
                    krate,
                    input,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            if let Some(output) = output {
                collect_items_from_type(
                    krate,
                    output,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
        }
        rustdoc_types::GenericArgs::ReturnTypeNotation => {}
    }
}

#[instrument(skip(krate, term, own_crate_key, prefix_match, seen, discovered))]
fn collect_items_from_term(
    krate: &rustdoc_types::Crate,
    term: &rustdoc_types::Term,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    if let rustdoc_types::Term::Type(ty) = term {
        collect_items_from_type(krate, ty, own_crate_key, prefix_match, seen, discovered);
    }
}

#[instrument(skip(krate, bound, own_crate_key, prefix_match, seen, discovered))]
fn collect_items_from_generic_bound(
    krate: &rustdoc_types::Crate,
    bound: &rustdoc_types::GenericBound,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    if let rustdoc_types::GenericBound::TraitBound {
        trait_,
        generic_params,
        ..
    } = bound
    {
        collect_items_from_path(krate, trait_, own_crate_key, prefix_match, seen, discovered);
        collect_items_from_generic_param_defs(
            krate,
            generic_params,
            own_crate_key,
            prefix_match,
            seen,
            discovered,
        );
    }
}

#[instrument(skip(krate, poly_trait, seen, discovered), fields(path = %poly_trait.trait_.path))]
fn collect_items_from_poly_trait(
    krate: &rustdoc_types::Crate,
    poly_trait: &rustdoc_types::PolyTrait,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    collect_items_from_path(
        krate,
        &poly_trait.trait_,
        own_crate_key,
        prefix_match,
        seen,
        discovered,
    );
    collect_items_from_generic_param_defs(
        krate,
        &poly_trait.generic_params,
        own_crate_key,
        prefix_match,
        seen,
        discovered,
    );
}

#[instrument(skip(krate, generic_params, seen, discovered), fields(param_count = generic_params.len()))]
fn collect_items_from_generic_param_defs(
    krate: &rustdoc_types::Crate,
    generic_params: &[rustdoc_types::GenericParamDef],
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    for generic in generic_params {
        match &generic.kind {
            rustdoc_types::GenericParamDefKind::Type {
                bounds, default, ..
            } => {
                for bound in bounds {
                    collect_items_from_generic_bound(
                        krate,
                        bound,
                        own_crate_key,
                        prefix_match,
                        seen,
                        discovered,
                    );
                }
                if let Some(default) = default {
                    collect_items_from_type(
                        krate,
                        default,
                        own_crate_key,
                        prefix_match,
                        seen,
                        discovered,
                    );
                }
            }
            rustdoc_types::GenericParamDefKind::Const { type_, .. } => {
                collect_items_from_type(
                    krate,
                    type_,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            rustdoc_types::GenericParamDefKind::Lifetime { .. } => {}
        }
    }
}

#[instrument(skip(krate, generics, seen, discovered), fields(param_count = generics.params.len(), predicate_count = generics.where_predicates.len()))]
fn collect_items_from_generics(
    krate: &rustdoc_types::Crate,
    generics: &rustdoc_types::Generics,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    collect_items_from_generic_param_defs(
        krate,
        &generics.params,
        own_crate_key,
        prefix_match,
        seen,
        discovered,
    );
    for predicate in &generics.where_predicates {
        collect_items_from_where_predicate(
            krate,
            predicate,
            own_crate_key,
            prefix_match,
            seen,
            discovered,
        );
    }
}

#[instrument(skip(krate, predicate, own_crate_key, prefix_match, seen, discovered))]
fn collect_items_from_where_predicate(
    krate: &rustdoc_types::Crate,
    predicate: &rustdoc_types::WherePredicate,
    own_crate_key: &str,
    prefix_match: bool,
    seen: &mut HashSet<String>,
    discovered: &mut Vec<ExtractedItem>,
) {
    match predicate {
        rustdoc_types::WherePredicate::BoundPredicate {
            type_,
            bounds,
            generic_params,
        } => {
            collect_items_from_type(krate, type_, own_crate_key, prefix_match, seen, discovered);
            for bound in bounds {
                collect_items_from_generic_bound(
                    krate,
                    bound,
                    own_crate_key,
                    prefix_match,
                    seen,
                    discovered,
                );
            }
            collect_items_from_generic_param_defs(
                krate,
                generic_params,
                own_crate_key,
                prefix_match,
                seen,
                discovered,
            );
        }
        rustdoc_types::WherePredicate::EqPredicate { lhs, rhs } => {
            collect_items_from_type(krate, lhs, own_crate_key, prefix_match, seen, discovered);
            collect_items_from_term(krate, rhs, own_crate_key, prefix_match, seen, discovered);
        }
        rustdoc_types::WherePredicate::LifetimePredicate { .. } => {}
    }
}

#[instrument(skip(ty))]
fn type_kind_name(ty: &rustdoc_types::Type) -> &'static str {
    match ty {
        rustdoc_types::Type::ResolvedPath(_) => "ResolvedPath",
        rustdoc_types::Type::DynTrait(_) => "DynTrait",
        rustdoc_types::Type::Generic(_) => "Generic",
        rustdoc_types::Type::Primitive(_) => "Primitive",
        rustdoc_types::Type::FunctionPointer(_) => "FunctionPointer",
        rustdoc_types::Type::Tuple(_) => "Tuple",
        rustdoc_types::Type::Slice(_) => "Slice",
        rustdoc_types::Type::Array { .. } => "Array",
        rustdoc_types::Type::ImplTrait(_) => "ImplTrait",
        rustdoc_types::Type::Infer => "Infer",
        rustdoc_types::Type::RawPointer { .. } => "RawPointer",
        rustdoc_types::Type::BorrowedRef { .. } => "BorrowedRef",
        rustdoc_types::Type::QualifiedPath { .. } => "QualifiedPath",
        _ => "Other",
    }
}

/// Map a rustdoc item to our [`ExtractedItemKind`], and extract generics info.
#[instrument(skip(item))]
fn classify_item(
    item: &rustdoc_types::Item,
) -> (ExtractedItemKind, bool, Vec<String>, Vec<String>) {
    match &item.inner {
        rustdoc_types::ItemEnum::Struct(s) => {
            let (lifetime_params, type_params) = extract_generic_params(&s.generics);
            let is_generic = !lifetime_params.is_empty() || !type_params.is_empty();
            (
                ExtractedItemKind::Struct,
                is_generic,
                lifetime_params,
                type_params,
            )
        }
        rustdoc_types::ItemEnum::Enum(e) => {
            let (lifetime_params, type_params) = extract_generic_params(&e.generics);
            let is_generic = !lifetime_params.is_empty() || !type_params.is_empty();
            (
                ExtractedItemKind::Enum,
                is_generic,
                lifetime_params,
                type_params,
            )
        }
        rustdoc_types::ItemEnum::Trait(t) => {
            let (lifetime_params, type_params) = extract_generic_params(&t.generics);
            let is_generic = !lifetime_params.is_empty() || !type_params.is_empty();
            (
                ExtractedItemKind::Trait,
                is_generic,
                lifetime_params,
                type_params,
            )
        }
        rustdoc_types::ItemEnum::TypeAlias(t) => {
            let (lifetime_params, type_params) = extract_generic_params(&t.generics);
            let is_generic = !lifetime_params.is_empty() || !type_params.is_empty();
            (
                ExtractedItemKind::TypeAlias,
                is_generic,
                lifetime_params,
                type_params,
            )
        }
        rustdoc_types::ItemEnum::Function(_) => {
            (ExtractedItemKind::Function, false, vec![], vec![])
        }
        rustdoc_types::ItemEnum::Macro(_) => (ExtractedItemKind::Macro, false, vec![], vec![]),
        rustdoc_types::ItemEnum::Constant { .. } => {
            (ExtractedItemKind::Constant, false, vec![], vec![])
        }
        rustdoc_types::ItemEnum::Module(_) => (ExtractedItemKind::Module, false, vec![], vec![]),
        _ => (ExtractedItemKind::Other, false, vec![], vec![]),
    }
}

/// Extract lifetime and type parameter names from a [`Generics`] block.
#[instrument(skip(generics))]
fn extract_generic_params(generics: &rustdoc_types::Generics) -> (Vec<String>, Vec<String>) {
    let mut lifetime_params = Vec::new();
    let mut type_params = Vec::new();

    for param in &generics.params {
        match &param.kind {
            rustdoc_types::GenericParamDefKind::Lifetime { .. } => {
                lifetime_params.push(param.name.clone())
            }
            rustdoc_types::GenericParamDefKind::Type { .. } => type_params.push(param.name.clone()),
            _ => {}
        }
    }

    (lifetime_params, type_params)
}
