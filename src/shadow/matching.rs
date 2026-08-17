//! Name matching and drift detection for shadow mirror compare.

use std::collections::HashMap;

use crate::rustdoc::{InventoryItemKind, RustdocItem};

pub fn counts_toward_shadow_coverage(item: &RustdocItem) -> bool {
    counts_toward_shadow_kind(item.kind)
}

pub fn counts_toward_shadow_kind(kind: InventoryItemKind) -> bool {
    !matches!(kind, InventoryItemKind::Other) && kind != InventoryItemKind::Trait
}

pub fn normalize_name(name: &str) -> String {
    to_snake_case(name).to_lowercase()
}

pub fn find_drift_match<'a>(
    target_item: &RustdocItem,
    shadow_names: &HashMap<String, Vec<&'a RustdocItem>>,
) -> Option<(&'a RustdocItem, f32)> {
    let target_norm = normalize_name(&target_item.name);
    let mut best: Option<(&RustdocItem, f32)> = None;

    for (shadow_norm, candidates) in shadow_names {
        let dist = edit_distance(&target_norm, shadow_norm);
        let max_len = target_norm.len().max(shadow_norm.len());
        if max_len == 0 {
            continue;
        }
        let confidence = 1.0 - (dist as f32 / max_len as f32);
        if confidence < 0.75 {
            continue;
        }
        for shadow_item in candidates {
            if shadow_item.kind != target_item.kind {
                continue;
            }
            if best.is_none_or(|(_, c)| confidence > c) {
                best = Some((shadow_item, confidence));
            }
        }
    }

    best
}

fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.char_indices() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    out
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate().take(n + 1) {
        *cell = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1])
            };
        }
    }
    dp[m][n]
}

/// Look up the method set for a type path, with bare-name suffix fallback.
pub fn methods_for_path(
    item_path: &str,
    methods: &std::collections::HashMap<String, std::collections::BTreeSet<String>>,
) -> std::collections::BTreeSet<String> {
    crate::rustdoc::methods_for_type_path(item_path, methods)
}

/// Look up trait impl bare names, falling back to bare trait name suffix match.
pub fn trait_impls_for_path<'a>(
    key: &str,
    map: &'a std::collections::HashMap<String, std::collections::BTreeSet<String>>,
) -> Option<&'a std::collections::BTreeSet<String>> {
    if let Some(found) = map.get(key) {
        return Some(found);
    }
    let bare = key.rsplit("::").next()?;
    let suffix = format!("::{bare}");
    let mut matches = map
        .iter()
        .filter(|(candidate, _)| candidate.as_str() == bare || candidate.ends_with(&suffix));
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(first.1)
}
