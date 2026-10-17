use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CordialConfig, CordialError, CordialResult, QualityReport, StoreLayout};

use super::item::assign_ids;
use super::{ItemSource, ItemState, WorklistItem};

use tracing::instrument;
const SCHEMA_VERSION: u32 = 1;

/// Facts that determine whether a persisted list still describes this run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct BuildRecord {
    schema_version: u32,
    cordial_version: String,
    config_fingerprint: String,
    generation: u64,
    built_at_unix_secs: u64,
    project_slug: String,
    tiers: Vec<String>,
}

/// Whether a list can still be used after a config or binary change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    /// Schema, binary, and effective config match.
    Current,
    /// The persisted format has changed.
    SchemaChanged,
    /// The running cordial version has changed.
    CordialChanged,
    /// Effective `cordial.toml` configuration has changed.
    ConfigChanged,
}

/// One generated list and the agent's claims against it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct Worklist {
    record: BuildRecord,
    items: Vec<WorklistItem>,
    cursor: Option<usize>,
}

impl Worklist {
    /// Build a fresh list from a completed quality run's checklists.
    ///
    /// The count check prevents a gate from publishing a list that omits
    /// actionable report items. The caller holds the store lock while building
    /// and writing so the generation is based on the latest list.
    #[instrument(level = "debug", skip(report, store, source, config))]
    pub fn build(
        report: &QualityReport,
        store: &StoreLayout,
        source: &dyn ItemSource,
        config: &CordialConfig,
    ) -> CordialResult<Self> {
        let mut items = Vec::new();
        for area in report.areas() {
            let from_area = source.items(area)?;
            if from_area.len() != area.open_items() {
                return Err(CordialError::invariant(format!(
                    "{}: report has {} open items but checklists have {}",
                    area.title(),
                    area.open_items(),
                    from_area.len()
                )));
            }
            items.extend(from_area);
        }
        assign_ids(&mut items)?;
        let generation = Self::read(store)?
            .map(|previous| previous.record.generation.saturating_add(1))
            .unwrap_or(1);
        let built_at_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| CordialError::invariant(error.to_string()))?
            .as_secs();
        let record = BuildRecord {
            schema_version: SCHEMA_VERSION,
            cordial_version: env!("CARGO_PKG_VERSION").to_string(),
            config_fingerprint: config_fingerprint(config)?,
            generation,
            built_at_unix_secs,
            project_slug: store.project_slug().clone(),
            tiers: vec!["all".to_string()],
        };
        Ok(Self {
            record,
            items,
            cursor: None,
        })
    }

    /// Read the current list, if one exists.
    #[instrument(level = "debug", skip(store))]
    pub fn read(store: &StoreLayout) -> CordialResult<Option<Self>> {
        let path = store.worklist_path();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| CordialError::json_parse(path.display().to_string(), error))
    }

    /// Atomically replace the stored list after a completed gate or cursor update.
    #[instrument(level = "debug", skip(self, store))]
    pub fn write(&self, store: &StoreLayout) -> CordialResult<()> {
        fs::create_dir_all(store.root())?;
        let target = store.worklist_path();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| CordialError::invariant(error.to_string()))?
            .as_nanos();
        let temp = store
            .root()
            .join(format!("worklist.json.tmp-{}-{stamp}", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let result = (|| -> CordialResult<()> {
            serde_json::to_writer_pretty(&mut file, self)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&temp, target)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }

    /// Check the hard validity signals; source edits do not invalidate a list.
    #[instrument(level = "debug", skip(self, config))]
    pub fn validity(&self, config: &CordialConfig) -> CordialResult<Validity> {
        if self.record.schema_version != SCHEMA_VERSION {
            return Ok(Validity::SchemaChanged);
        }
        if self.record.cordial_version != env!("CARGO_PKG_VERSION") {
            return Ok(Validity::CordialChanged);
        }
        if self.record.config_fingerprint != config_fingerprint(config)? {
            return Ok(Validity::ConfigChanged);
        }
        Ok(Validity::Current)
    }

    /// Count items still present for the gate, including deferred items.
    #[instrument(level = "trace", skip(self))]
    pub fn open_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| *item.state() != ItemState::Done)
            .count()
    }

    /// Item at the cursor, if it has not moved past the end.
    #[instrument(level = "trace", skip(self))]
    pub fn current(&self) -> Option<&WorklistItem> {
        self.cursor.and_then(|index| self.items.get(index))
    }

    /// Place a fresh list's cursor on its first available item.
    #[instrument(level = "trace", skip(self))]
    pub fn activate(&mut self) -> bool {
        if self.cursor.is_some() {
            return false;
        }
        self.cursor = self
            .items
            .iter()
            .position(|item| *item.state() == ItemState::Open);
        self.cursor.is_some()
    }
}

#[instrument(level = "debug", skip(config))]
fn config_fingerprint(config: &CordialConfig) -> CordialResult<String> {
    let bytes = serde_json::to_vec(config)?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}
