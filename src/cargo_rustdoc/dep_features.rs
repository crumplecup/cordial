//! Cargo feature resolution for impl coverage feature probes.

use std::collections::{BTreeMap, HashSet, VecDeque};

use tracing::instrument;

use crate::error::{CordialError, CordialResult};

/// How the reference workspace currently depends on an upstream crate.
#[derive(Debug, Clone, Default, PartialEq, Eq, derive_new::new, derive_getters::Getters)]
pub struct DepBuildConfig {
    activated_features: Vec<String>,
    #[getter(copy)]
    uses_default_features: bool,
}

/// Resolve dependency features from a workspace member's `Cargo.toml`.
#[instrument(level = "debug", err(level = "warn"))]
pub fn collect_member_dep_build_config(
    reference_workspace: &std::path::Path,
    member_crate_name: &str,
    crate_name: &str,
) -> CordialResult<DepBuildConfig> {
    let meta = cargo_metadata::MetadataCommand::new()
        .manifest_path(reference_workspace.join("Cargo.toml"))
        .exec()
        .map_err(CordialError::cargo_metadata)?;

    let member_pkg = meta
        .packages
        .iter()
        .find(|pkg| pkg.name == member_crate_name)
        .ok_or_else(|| {
            CordialError::invariant(format!(
                "workspace package `{member_crate_name}` not found in cargo metadata"
            ))
        })?;

    let normalized = crate_name.replace('-', "_");
    let dep = member_pkg
        .dependencies
        .iter()
        .find(|dep| {
            dep.name == crate_name
                || dep.name.replace('-', "_") == normalized
                || dep.rename.as_deref().is_some_and(|rename| {
                    rename == crate_name || rename.replace('-', "_") == normalized
                })
        })
        .ok_or_else(|| {
            CordialError::invariant(format!(
                "dependency '{crate_name}' not found in `{member_crate_name}` package metadata"
            ))
        })?;

    let mut activated_features = dep.features.clone();
    activated_features.sort();
    activated_features.dedup();

    Ok(DepBuildConfig::new(
        activated_features,
        dep.uses_default_features,
    ))
}

/// Returns `(available_serde_features, expanded_activated_features)`.
#[instrument(level = "debug", err(level = "warn"))]
pub fn collect_dep_serde_features(
    reference_workspace: &std::path::Path,
    crate_name: &str,
    activated: &[&str],
    uses_default_features: bool,
) -> CordialResult<(Vec<String>, Vec<String>)> {
    let meta = cargo_metadata::MetadataCommand::new()
        .manifest_path(reference_workspace.join("Cargo.toml"))
        .features(cargo_metadata::CargoOpt::AllFeatures)
        .exec()
        .map_err(CordialError::cargo_metadata)?;

    let normalized = crate_name.replace('-', "_");
    let pkg = meta
        .packages
        .iter()
        .find(|pkg| pkg.name == crate_name || pkg.name.replace('-', "_") == normalized)
        .ok_or_else(|| {
            CordialError::invariant(format!(
                "package '{crate_name}' not found in workspace metadata"
            ))
        })?;

    let available = collect_available_serde_features(&pkg.features);
    let mut activated_owned: Vec<String> = activated
        .iter()
        .map(|feature| (*feature).to_string())
        .collect();
    if uses_default_features && pkg.features.contains_key("default") {
        activated_owned.push("default".to_string());
    }
    let expanded_activated = expand_same_package_features(&pkg.features, &activated_owned);
    Ok((available, expanded_activated))
}

#[instrument(level = "debug", skip(features))]
fn collect_available_serde_features(features: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    const KEYWORDS: &[&str] = &["serde", "schemars", "schema", "json"];
    let mut available: Vec<String> = features
        .keys()
        .filter(|name| {
            name.as_str() != "default" && feature_reaches_external_support(name, features, KEYWORDS)
        })
        .cloned()
        .collect();
    available.sort();
    available
}

#[instrument(level = "debug", skip(features))]
fn expand_same_package_features(
    features: &BTreeMap<String, Vec<String>>,
    activated: &[String],
) -> Vec<String> {
    let mut expanded: HashSet<String> = activated.iter().cloned().collect();
    let mut queue: VecDeque<String> = activated.iter().cloned().collect();

    while let Some(feature) = queue.pop_front() {
        let Some(edges) = features.get(&feature) else {
            continue;
        };
        for edge in edges {
            if !edge.contains(':') && !edge.contains('/') && expanded.insert(edge.clone()) {
                queue.push_back(edge.clone());
            }
        }
    }

    let mut expanded_features: Vec<String> = expanded.into_iter().collect();
    expanded_features.sort();
    expanded_features
}

#[instrument(level = "debug", skip(features))]
fn feature_reaches_external_support(
    root: &str,
    features: &BTreeMap<String, Vec<String>>,
    keywords: &[&str],
) -> bool {
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = std::iter::once(root.to_string()).collect();

    while let Some(feature) = queue.pop_front() {
        if !seen.insert(feature.clone()) {
            continue;
        }

        let feature_lc = feature.to_lowercase();
        if keywords.iter().any(|kw| feature_lc.contains(kw)) {
            return true;
        }

        let Some(edges) = features.get(&feature) else {
            continue;
        };

        for edge in edges {
            if !edge.contains(':') && !edge.contains('/') {
                queue.push_back(edge.clone());
            }
        }
    }

    false
}
