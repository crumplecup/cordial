//! Per-type feature probe results for impl gap classification.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use tracing::instrument;

use crate::cargo_rustdoc::{collect_dep_serde_features, collect_member_dep_build_config};
use crate::error::CordialResult;
use crate::plugin::{WorkspaceHub, discover_workspace_hub};
use crate::rustdoc::{TraitPrereqs, collect_trait_prereqs_for_inventory, parse_rustdoc_json};
use crate::session::RunFilter;

/// Per-type feature probe result used for actionable impl-gap reporting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeFeatureProbe {
    pub feature_crate: String,
    pub candidate_unlock_features: Vec<String>,
    pub probed_prereqs: Option<TraitPrereqs>,
}

/// Hub crate name for feature-probe dependency resolution.
#[instrument(level = "debug", skip(hub))]
pub fn hub_crate_name(hub: WorkspaceHub) -> Option<&'static str> {
    match hub {
        WorkspaceHub::Elicitation => Some("elicitation"),
        WorkspaceHub::Amenable => Some("amenable"),
        WorkspaceHub::Homecoming => Some("homecoming"),
        WorkspaceHub::Unknown => None,
    }
}

/// Load per-type feature probes for a report crate, using cached or live probe rustdoc when available.
#[instrument(level = "info", skip(filter), err(level = "warn"))]
pub fn load_crate_feature_probes(
    project_root: &Path,
    store_root: &Path,
    filter: &dyn RunFilter,
    report_crate: &str,
    type_paths: &[String],
) -> CordialResult<HashMap<String, TypeFeatureProbe>> {
    let hub = discover_workspace_hub(project_root, filter)?;
    let Some(hub_member) = hub_crate_name(hub) else {
        return Ok(HashMap::new());
    };

    let dep_config =
        collect_member_dep_build_config(project_root, hub_member, report_crate).unwrap_or_default();

    let activated_refs: Vec<&str> = dep_config
        .activated_features
        .iter()
        .map(String::as_str)
        .collect();
    let (available_features, expanded_activated_features) = collect_dep_serde_features(
        project_root,
        report_crate,
        &activated_refs,
        dep_config.uses_default_features,
    )?;
    let expanded_activated: BTreeSet<String> = expanded_activated_features.into_iter().collect();
    let candidate_unlock_features: Vec<String> = available_features
        .into_iter()
        .filter(|feature| !expanded_activated.contains(feature))
        .collect();

    build_type_feature_probes(
        project_root,
        store_root,
        report_crate,
        type_paths,
        &dep_config.activated_features,
        &candidate_unlock_features,
        dep_config.uses_default_features,
    )
}

#[instrument(level = "debug", err(level = "warn"))]
pub fn build_type_feature_probes(
    project_root: &Path,
    store_root: &Path,
    report_crate_name: &str,
    type_paths: &[String],
    activated_features: &[String],
    candidate_unlock_features: &[String],
    uses_default_features: bool,
) -> CordialResult<HashMap<String, TypeFeatureProbe>> {
    if candidate_unlock_features.is_empty() {
        return Ok(HashMap::new());
    }

    let mut probe_features: BTreeSet<String> = activated_features
        .iter()
        .filter(|feature| feature.as_str() != "default")
        .cloned()
        .collect();
    probe_features.extend(candidate_unlock_features.iter().cloned());
    let probe_owned: Vec<String> = probe_features.into_iter().collect();
    let probe_refs: Vec<&str> = probe_owned.iter().map(String::as_str).collect();

    let probed_map = match resolve_probe_prereqs(
        project_root,
        store_root,
        report_crate_name,
        &probe_refs,
        uses_default_features,
    ) {
        Ok(map) => Some(map),
        Err(error) => {
            tracing::warn!(
                report_crate = report_crate_name,
                error = %error,
                "could not collect probed trait prereqs"
            );
            None
        }
    };

    let mut probes = HashMap::new();
    for type_path in type_paths {
        let probed_prereqs = probed_map
            .as_ref()
            .and_then(|map| map.get(type_path).cloned());
        probes.insert(
            type_path.clone(),
            TypeFeatureProbe {
                feature_crate: report_crate_name.to_string(),
                candidate_unlock_features: candidate_unlock_features.to_vec(),
                probed_prereqs,
            },
        );
    }

    Ok(probes)
}

#[instrument(level = "debug", err(level = "warn"))]
fn resolve_probe_prereqs(
    project_root: &Path,
    store_root: &Path,
    report_crate_name: &str,
    probe_features: &[&str],
    uses_default_features: bool,
) -> CordialResult<HashMap<String, TraitPrereqs>> {
    let path = probe_rustdoc_cache_path(store_root, report_crate_name);
    if path.is_file() {
        return Ok(collect_trait_prereqs_for_inventory(&parse_rustdoc_json(
            &path,
            report_crate_name,
        )?));
    }

    if !should_build_probe_rustdoc() {
        return Err(crate::error::CordialError::invariant(
            "probe rustdoc cache miss and live probe builds disabled".to_string(),
        ));
    }

    let mut features: Vec<String> = probe_features.iter().map(|s| (*s).to_string()).collect();
    if uses_default_features && !features.iter().any(|feature| feature == "default") {
        features.push("default".to_string());
    }
    let feature_refs: Vec<&str> = features.iter().map(String::as_str).collect();
    let json_path =
        crate::cargo_rustdoc::run_cargo_rustdoc(project_root, report_crate_name, &feature_refs)?;
    let cache_path = probe_rustdoc_cache_path(store_root, report_crate_name);
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&json_path, &cache_path)?;
    Ok(collect_trait_prereqs_for_inventory(&parse_rustdoc_json(
        &cache_path,
        report_crate_name,
    )?))
}

#[instrument(level = "debug")]
fn probe_rustdoc_cache_path(store_root: &Path, crate_name: &str) -> PathBuf {
    let normalized = crate_name.replace('-', "_");
    store_root
        .join("cache/rustdoc-probe")
        .join(format!("{normalized}.json"))
}

#[instrument(level = "debug")]
fn should_build_probe_rustdoc() -> bool {
    std::env::var("CORDIAL_PROBE_FEATURES")
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
}
