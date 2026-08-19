//! [`ShadowReport`] construction — diff upstream inventory against shadow inventory.

use std::collections::{BTreeSet, HashMap, HashSet};

use tracing::instrument;

use crate::rustdoc::{
    ElicitCompleteSet, InventoryItemKind, RustdocInventory, RustdocItem, TraitPrereqs,
    collect_elicit_complete_from_inventory, collect_trait_prereqs_for_inventory,
};

use super::matching::{
    counts_toward_shadow_coverage, find_drift_match, methods_for_path, normalize_name,
    trait_impls_for_path,
};
use super::types::{
    ShadowBuildMaps, ShadowReport, ShadowRow, ShadowStatus, TraitImplCoverage, TypeMethodCoverage,
};
use super::verification::{
    shadow_can_be_direct, shadow_impl_status, shadow_missing_external_traits,
    shadow_missing_our_traits, shadow_verification_gap,
};

/// Build a [`ShadowReport`] by diffing `target` against `shadow`.
#[instrument(
    level = "debug",
    skip(target, shadow, shadow_complete, shadow_prereqs, maps)
)]
pub fn build_shadow_report(
    target: &RustdocInventory,
    shadow: &RustdocInventory,
    shadow_complete: &ElicitCompleteSet,
    shadow_prereqs: &HashMap<String, TraitPrereqs>,
    maps: &ShadowBuildMaps<'_>,
) -> ShadowReport {
    let shadow_index = index_shadow_items(shadow);
    let mut rows = match_target_rows(target, &shadow_index, shadow_complete, shadow_prereqs);
    append_extra_shadow_rows(shadow, &mut rows);
    rows.sort_by(|left, right| left.item_path.cmp(&right.item_path));

    let (covered_count, missing_count, extra_count, drifted_count, coverage_pct) =
        tally_coverage(&rows, target);
    let verification_gap_count = rows
        .iter()
        .filter(|row| shadow_verification_gap(row))
        .count();
    let method_coverage = collect_method_coverage(&rows, maps);
    let missing_type_methods = collect_missing_type_methods(&rows, maps);
    let trait_coverage = collect_trait_coverage(&rows, maps);

    ShadowReport {
        target_crate: target.crate_name.clone(),
        shadow_crate: shadow.crate_name.clone(),
        rows,
        covered_count,
        missing_count,
        extra_count,
        drifted_count,
        coverage_pct,
        verification_gap_count,
        method_coverage,
        missing_type_methods,
        trait_coverage,
    }
}

struct ShadowItemIndex<'a> {
    by_name: HashMap<&'a str, Vec<&'a RustdocItem>>,
    normalized: HashMap<String, Vec<&'a RustdocItem>>,
}

#[instrument(level = "debug", skip(shadow))]
fn index_shadow_items(shadow: &RustdocInventory) -> ShadowItemIndex<'_> {
    let mut by_name: HashMap<&str, Vec<&RustdocItem>> = HashMap::new();
    let mut normalized: HashMap<String, Vec<&RustdocItem>> = HashMap::new();
    for item in &shadow.items {
        if !counts_toward_shadow_coverage(item) {
            continue;
        }
        by_name.entry(item.name.as_str()).or_default().push(item);
        normalized
            .entry(normalize_name(&item.name))
            .or_default()
            .push(item);
    }
    ShadowItemIndex {
        by_name,
        normalized,
    }
}

#[instrument(level = "debug", skip(target, shadow, shadow_complete, shadow_prereqs))]
fn match_target_rows(
    target: &RustdocInventory,
    shadow: &ShadowItemIndex<'_>,
    shadow_complete: &ElicitCompleteSet,
    shadow_prereqs: &HashMap<String, TraitPrereqs>,
) -> Vec<ShadowRow> {
    let mut rows = Vec::new();
    for target_item in &target.items {
        if !counts_toward_shadow_coverage(target_item) {
            continue;
        }
        let exact = shadow
            .by_name
            .get(target_item.name.as_str())
            .and_then(|candidates| {
                candidates
                    .iter()
                    .find(|candidate| candidate.kind == target_item.kind)
                    .or_else(|| candidates.first())
                    .copied()
            });
        if let Some(shadow_item) = exact {
            rows.push(row_for_match(
                target_item,
                shadow_item,
                ShadowStatus::Covered,
                String::new(),
                String::new(),
                shadow_complete,
                shadow_prereqs,
            ));
        } else if let Some((shadow_item, confidence)) =
            find_drift_match(target_item, &shadow.normalized)
        {
            rows.push(row_for_match(
                target_item,
                shadow_item,
                ShadowStatus::Drifted,
                shadow_item.path.clone(),
                format!("{confidence:.2}"),
                shadow_complete,
                shadow_prereqs,
            ));
        } else {
            rows.push(missing_row(target_item));
        }
    }
    rows
}

#[instrument(level = "debug", skip(target_item))]
fn missing_row(target_item: &crate::rustdoc::RustdocItem) -> ShadowRow {
    ShadowRow {
        item_path: target_item.path.clone(),
        item_kind: target_item.kind,
        status: ShadowStatus::Missing,
        shadow_item: String::new(),
        drift_confidence: String::new(),
        shadow_elicit_impl: String::new(),
        shadow_can_be_direct: String::new(),
        shadow_missing_external_traits: String::new(),
        shadow_missing_our_traits: String::new(),
        notes: String::new(),
    }
}

#[instrument(level = "debug", skip(shadow, rows))]
fn append_extra_shadow_rows(shadow: &RustdocInventory, rows: &mut Vec<ShadowRow>) {
    let matched: HashSet<String> = rows
        .iter()
        .filter(|row| matches!(row.status, ShadowStatus::Covered | ShadowStatus::Drifted))
        .map(|row| row.shadow_item.clone())
        .collect();
    for shadow_item in &shadow.items {
        if !counts_toward_shadow_coverage(shadow_item) || matched.contains(&shadow_item.path) {
            continue;
        }
        rows.push(ShadowRow {
            item_path: shadow_item.path.clone(),
            item_kind: shadow_item.kind,
            status: ShadowStatus::Extra,
            shadow_item: String::new(),
            drift_confidence: String::new(),
            shadow_elicit_impl: String::new(),
            shadow_can_be_direct: String::new(),
            shadow_missing_external_traits: String::new(),
            shadow_missing_our_traits: String::new(),
            notes: "in shadow, not in target".to_string(),
        });
    }
}

#[instrument(level = "debug", skip(rows, target))]
fn tally_coverage(
    rows: &[ShadowRow],
    target: &RustdocInventory,
) -> (usize, usize, usize, usize, f64) {
    let covered_count = rows
        .iter()
        .filter(|row| row.status == ShadowStatus::Covered)
        .count();
    let missing_count = rows
        .iter()
        .filter(|row| row.status == ShadowStatus::Missing)
        .count();
    let extra_count = rows
        .iter()
        .filter(|row| row.status == ShadowStatus::Extra)
        .count();
    let drifted_count = rows
        .iter()
        .filter(|row| row.status == ShadowStatus::Drifted)
        .count();
    let total_target = target
        .items
        .iter()
        .filter(|item| counts_toward_shadow_coverage(item))
        .count();
    let coverage_pct = if total_target == 0 {
        100.0
    } else {
        (covered_count + drifted_count) as f64 / total_target as f64 * 100.0
    };
    (
        covered_count,
        missing_count,
        extra_count,
        drifted_count,
        coverage_pct,
    )
}

#[instrument(level = "debug", skip(rows, maps))]
fn collect_method_coverage(
    rows: &[ShadowRow],
    maps: &ShadowBuildMaps<'_>,
) -> Vec<TypeMethodCoverage> {
    if maps.target_methods.is_empty() && maps.shadow_methods.is_empty() {
        return Vec::new();
    }
    rows.iter()
        .filter(|row| {
            matches!(row.status, ShadowStatus::Covered | ShadowStatus::Drifted)
                && row.item_kind.is_type()
        })
        .map(|row| diff_type_methods(&row.item_path, &row.shadow_item, maps))
        .collect()
}

#[instrument(level = "debug", skip(rows, maps))]
fn collect_missing_type_methods(
    rows: &[ShadowRow],
    maps: &ShadowBuildMaps<'_>,
) -> Vec<TypeMethodCoverage> {
    if maps.target_methods.is_empty() {
        return Vec::new();
    }
    rows.iter()
        .filter(|row| row.status == ShadowStatus::Missing && row.item_kind.is_type())
        .filter_map(|row| {
            let upstream = methods_for_path(&row.item_path, maps.target_methods);
            if upstream.is_empty() {
                return None;
            }
            let mut missing: Vec<String> = upstream.into_iter().collect();
            missing.sort();
            Some(TypeMethodCoverage {
                upstream_type: row.item_path.clone(),
                shadow_type: String::new(),
                covered: Vec::new(),
                missing,
                extra: Vec::new(),
            })
        })
        .collect()
}

#[instrument(level = "debug", skip(rows, maps))]
fn collect_trait_coverage(
    rows: &[ShadowRow],
    maps: &ShadowBuildMaps<'_>,
) -> Vec<TraitImplCoverage> {
    let shadow_bare_names: HashSet<String> = rows
        .iter()
        .filter(|row| matches!(row.status, ShadowStatus::Covered | ShadowStatus::Drifted))
        .filter_map(|row| row.item_path.rsplit("::").next().map(str::to_owned))
        .collect();
    rows.iter()
        .filter(|row| {
            row.status == ShadowStatus::Missing && row.item_kind == InventoryItemKind::Trait
        })
        .filter_map(|row| {
            let empty = BTreeSet::new();
            let target_impls = maps
                .target_trait_impls
                .get(&row.item_path)
                .unwrap_or(&empty);
            let shadow_impls =
                trait_impls_for_path(&row.item_path, maps.shadow_trait_impls).unwrap_or(&empty);
            let mut missing_on_shadow: Vec<String> = target_impls
                .iter()
                .filter(|name| shadow_bare_names.contains(*name))
                .filter(|name| !shadow_impls.contains(*name))
                .cloned()
                .collect();
            let mut covered_on_shadow: Vec<String> = target_impls
                .iter()
                .filter(|name| shadow_impls.contains(*name))
                .cloned()
                .collect();
            missing_on_shadow.sort();
            covered_on_shadow.sort();
            if missing_on_shadow.is_empty() && covered_on_shadow.is_empty() {
                return None;
            }
            Some(TraitImplCoverage {
                trait_path: row.item_path.clone(),
                missing_on_shadow,
                covered_on_shadow,
            })
        })
        .collect()
}

#[instrument(level = "debug", skip(maps))]
fn diff_type_methods(
    upstream_type: &str,
    shadow_type: &str,
    maps: &ShadowBuildMaps<'_>,
) -> TypeMethodCoverage {
    let upstream = methods_for_path(upstream_type, maps.target_methods);
    let shadow = methods_for_path(shadow_type, maps.shadow_methods);
    let mut covered: Vec<String> = upstream.intersection(&shadow).cloned().collect();
    let mut missing: Vec<String> = upstream.difference(&shadow).cloned().collect();
    let mut extra: Vec<String> = shadow.difference(&upstream).cloned().collect();
    covered.sort();
    missing.sort();
    extra.sort();
    TypeMethodCoverage {
        upstream_type: upstream_type.to_string(),
        shadow_type: shadow_type.to_string(),
        covered,
        missing,
        extra,
    }
}

/// Convenience wrapper that derives shadow complete/prereqs from the shadow inventory.
#[instrument(level = "debug", skip(target, shadow))]
pub fn build_shadow_report_from_inventories(
    target: &RustdocInventory,
    shadow: &RustdocInventory,
) -> ShadowReport {
    build_shadow_report_from_inventories_with_maps(target, shadow, &ShadowBuildMaps::empty())
}

#[instrument(level = "debug", skip(target, shadow, maps))]
pub fn build_shadow_report_from_inventories_with_maps(
    target: &RustdocInventory,
    shadow: &RustdocInventory,
    maps: &ShadowBuildMaps<'_>,
) -> ShadowReport {
    let shadow_complete = collect_elicit_complete_from_inventory(shadow);
    let shadow_prereqs = collect_trait_prereqs_for_inventory(shadow);
    build_shadow_report(target, shadow, &shadow_complete, &shadow_prereqs, maps)
}

#[instrument(
    level = "debug",
    skip(target_item, shadow_item, status, shadow_complete, shadow_prereqs)
)]
fn row_for_match(
    target_item: &crate::rustdoc::RustdocItem,
    shadow_item: &crate::rustdoc::RustdocItem,
    status: ShadowStatus,
    shadow_item_path: String,
    drift_confidence: String,
    shadow_complete: &ElicitCompleteSet,
    shadow_prereqs: &HashMap<String, TraitPrereqs>,
) -> ShadowRow {
    let notes = if status == ShadowStatus::Drifted {
        "probable rename".to_string()
    } else {
        String::new()
    };
    ShadowRow {
        item_path: target_item.path.clone(),
        item_kind: target_item.kind,
        status,
        shadow_item: if shadow_item_path.is_empty() {
            shadow_item.path.clone()
        } else {
            shadow_item_path
        },
        drift_confidence,
        shadow_elicit_impl: shadow_impl_status(shadow_item, shadow_complete)
            .as_str()
            .to_string(),
        shadow_can_be_direct: shadow_can_be_direct(shadow_item, shadow_prereqs),
        shadow_missing_external_traits: shadow_missing_external_traits(shadow_item, shadow_prereqs),
        shadow_missing_our_traits: shadow_missing_our_traits(shadow_item, shadow_prereqs),
        notes,
    }
}
