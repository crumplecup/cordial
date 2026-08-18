//! Climb containment and re-export edges to inherit rustdoc stability markers.

use std::collections::{HashMap, HashSet};

use tracing::instrument;

use crate::rustdoc::stability::item_attrs_are_unstable;

type ModuleAncestryMap = HashMap<rustdoc_types::Id, rustdoc_types::Id>;
type ReexportingUseMap = HashMap<rustdoc_types::Id, Vec<rustdoc_types::Id>>;

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
pub(super) fn rustdoc_item_is_unstable(
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
