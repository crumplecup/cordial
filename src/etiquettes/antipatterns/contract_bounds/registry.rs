//! Load `amenable dump-registry` contract records for the unnamed-bound rule.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use crate::error::{CordialError, CordialResult};
use crate::store::StoreLayout;

use super::index::{ContractRecordDump, RegistryDump};

use tracing::instrument;
const AMENABLE_DUMP_REGISTRY_FEATURES: &str = "creusot,verus";

static CONTRACT_RECORDS_CACHE: Mutex<Option<(PathBuf, Vec<ContractRecordDump>)>> = Mutex::new(None);

/// Load contract records for the unnamed-contract-bound rule.
#[instrument(level = "info")]
pub fn fetch_contract_records(workspace_root: &Path, store_root: &Path) -> Vec<ContractRecordDump> {
    let cache_key = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());

    if let Ok(cache) = CONTRACT_RECORDS_CACHE.lock()
        && let Some((key, records)) = cache.as_ref()
        && *key == cache_key
    {
        return records.clone();
    }

    let slug = crate::store::project_slug_from_path(workspace_root);
    let store = StoreLayout::from_root(store_root, slug);
    let dump_path = store.cache_dir().join("amenable-registry.dump.json");

    let records = if dump_path.is_file() {
        load_registry_dump(&dump_path).unwrap_or_default()
    } else if workspace_has_amenable(workspace_root) {
        if run_amenable_dump_registry(workspace_root, &dump_path).is_ok() {
            load_registry_dump(&dump_path).unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    if let Ok(mut cache) = CONTRACT_RECORDS_CACHE.lock() {
        *cache = Some((cache_key, records.clone()));
    }
    records
}

#[instrument(level = "info", skip(path), err(level = "warn"))]
fn load_registry_dump(path: &Path) -> CordialResult<Vec<ContractRecordDump>> {
    let content = std::fs::read_to_string(path)?;
    let dump: RegistryDump = serde_json::from_str(&content)
        .map_err(|err| CordialError::json_parse(path.display().to_string(), err))?;
    Ok(dump.contract_records)
}

#[instrument(level = "debug")]
fn workspace_has_amenable(workspace_root: &Path) -> bool {
    cargo_metadata::MetadataCommand::new()
        .current_dir(workspace_root)
        .exec()
        .ok()
        .is_some_and(|meta| {
            meta.packages
                .iter()
                .any(|package| package.name == "amenable")
        })
}

#[instrument(level = "info", skip(workspace), err(level = "warn"))]
fn run_amenable_dump_registry(workspace: &Path, out_path: &Path) -> CordialResult<()> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let status = Command::new("cargo")
        .current_dir(workspace)
        .arg("run")
        .arg("-p")
        .arg("amenable")
        .arg("--features")
        .arg(AMENABLE_DUMP_REGISTRY_FEATURES)
        .arg("--")
        .arg("dump-registry")
        .arg("--out")
        .arg(out_path)
        .status()
        .map_err(|err| {
            CordialError::invariant(format!("failed to run amenable dump-registry: {err}"))
        })?;

    if !status.success() {
        return Err(CordialError::invariant(format!(
            "amenable dump-registry exited with {status}"
        )));
    }
    if !out_path.is_file() {
        return Err(CordialError::invariant(format!(
            "amenable dump-registry did not write {}",
            out_path.display()
        )));
    }
    Ok(())
}
