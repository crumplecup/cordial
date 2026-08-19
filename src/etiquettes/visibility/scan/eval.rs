//! Branching-floor evaluation and crate-file digest.

use sha2::{Digest, Sha256};

use super::BranchingCache;
use super::tree::{ModuleNode, collect_files, external_name_count, public_path_mods};
use crate::etiquettes::visibility::types::VisibilityThresholds;

use tracing::instrument;
/// How the scanner applies [`VisibilityThresholds::min_module_names`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VisibilityEval {
    /// `prefer_root`: thin checks use `min_module_names`. An oversized flat
    /// root is accepted — that is the preferred resolution.
    Normal,
    /// `prefer_root = false`: thin checks use `floor`, which starts at
    /// `min_module_names` and drops as the largest undersized modules are
    /// peeled back off root.
    Branching { floor: usize },
}

impl VisibilityEval {
    #[instrument(level = "debug", skip(self, thresholds))]
    pub(super) fn thin_floor(self, thresholds: VisibilityThresholds) -> usize {
        match self {
            Self::Normal => thresholds.min_module_names,
            Self::Branching { floor } => floor,
        }
    }
}

#[instrument(level = "debug", skip(root, thresholds, cached))]
pub(super) fn resolve_eval(
    root: &ModuleNode,
    thresholds: VisibilityThresholds,
    cached: Option<BranchingCache>,
) -> (VisibilityEval, Option<BranchingCache>) {
    if thresholds.prefer_root {
        return (VisibilityEval::Normal, None);
    }
    let digest = tree_digest(root);
    if let Some(cache) = cached.filter(|cache| cache.digest == digest) {
        return (
            VisibilityEval::Branching { floor: cache.floor },
            Some(cache),
        );
    }
    let floor = peel_branching_floor(root, thresholds);
    let cache = BranchingCache { digest, floor };
    (VisibilityEval::Branching { floor }, Some(cache))
}

/// Peel the largest undersized public-path modules off a conceptually
/// flattened root until remaining names sit under `max_crate_names_for_flat`.
/// Modules that already meet `min_module_names` stay put and do not move the
/// floor. The thin floor follows each peeled module's size (10 → 9 → 7 → 6).
#[instrument(level = "debug", skip(root, thresholds))]
fn peel_branching_floor(root: &ModuleNode, thresholds: VisibilityThresholds) -> usize {
    let mut floor = thresholds.min_module_names;
    let mods = public_path_mods(root);
    let mut remaining = external_name_count(root);
    let mut reserved: Vec<&str> = Vec::new();
    for module in &mods {
        let size = external_name_count(module);
        if size < thresholds.min_module_names {
            continue;
        }
        if reserved
            .iter()
            .any(|parent| is_path_under(&module.path, parent))
        {
            continue;
        }
        remaining = remaining.saturating_sub(size);
        reserved.push(&module.path);
    }
    if remaining < thresholds.max_crate_names_for_flat {
        return floor;
    }
    let mut candidates: Vec<&ModuleNode> = mods
        .iter()
        .copied()
        .filter(|module| {
            let size = external_name_count(module);
            size > 0
                && size < thresholds.min_module_names
                && !reserved
                    .iter()
                    .any(|parent| is_path_under(&module.path, parent))
        })
        .collect();
    candidates.sort_by(|left, right| {
        external_name_count(right)
            .cmp(&external_name_count(left))
            .then_with(|| left.path.cmp(&right.path))
    });
    for candidate in candidates {
        if remaining < thresholds.max_crate_names_for_flat {
            break;
        }
        if reserved
            .iter()
            .any(|parent| is_path_under(&candidate.path, parent))
        {
            continue;
        }
        let size = external_name_count(candidate);
        remaining = remaining.saturating_sub(size);
        floor = size;
        reserved.push(&candidate.path);
    }
    floor
}

#[instrument(level = "trace", skip(path), ret)]
fn is_path_under(path: &str, parent: &str) -> bool {
    path == parent || path.starts_with(&format!("{parent}::"))
}

#[instrument(level = "debug", skip(root))]
fn tree_digest(root: &ModuleNode) -> String {
    let mut files = Vec::new();
    collect_files(root, &mut files);
    files.sort();
    files.dedup();
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.to_string_lossy().as_bytes());
        if let Ok(bytes) = std::fs::read(&file) {
            hasher.update(&bytes);
        }
    }
    format!("{:x}", hasher.finalize())
}
