use std::collections::BTreeSet;
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
    #[serde(default = "legacy_tiers")]
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
    #[serde(default = "default_max_passes")]
    max_passes: u32,
}

/// The result of applying one claim to a worklist item.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ClaimChange {
    id: String,
    item_line: String,
    was: ItemState,
    now: ItemState,
}

impl Worklist {
    /// Mutable items for reconciliation after a completed gate.
    #[instrument(level = "trace", skip(self))]
    pub(super) fn items_mut(&mut self) -> &mut [WorklistItem] {
        &mut self.items
    }

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
        Self::build_with_tiers(report, store, source, config, legacy_tiers())
    }

    /// Build a list that records the effective tier selection of this gate.
    #[instrument(level = "debug", skip(report, store, source, config, tiers))]
    pub fn build_with_tiers(
        report: &QualityReport,
        store: &StoreLayout,
        source: &dyn ItemSource,
        config: &CordialConfig,
        tiers: Vec<String>,
    ) -> CordialResult<Self> {
        let mut items = Vec::new();
        for area in report.areas() {
            let from_area = source.items(area)?;
            if from_area.len() != area.open_items() {
                return Err(CordialError::area_open_item_count_mismatch(
                    area.title(),
                    area.open_items(),
                    from_area.len(),
                ));
            }
            items.extend(from_area);
        }
        assign_ids(&mut items)?;
        let generation = Self::read(store)?
            .map(|previous| previous.record.generation.saturating_add(1))
            .unwrap_or(1);
        let built_at_unix_secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let record = BuildRecord {
            schema_version: SCHEMA_VERSION,
            cordial_version: env!("CARGO_PKG_VERSION").to_string(),
            config_fingerprint: config_fingerprint(config)?,
            generation,
            built_at_unix_secs,
            project_slug: store.project_slug().clone(),
            tiers,
        };
        Ok(Self {
            record,
            items,
            cursor: None,
            max_passes: config.worklist().max_passes().max(1),
        })
    }

    /// Preserve prior items whose checklist was not verified in this run.
    #[instrument(level = "debug", skip(self, previous, verified_checklists))]
    pub fn carry_unverified(
        &mut self,
        previous: &Self,
        verified_checklists: &BTreeSet<String>,
    ) -> usize {
        let carried: Vec<_> = previous
            .items()
            .iter()
            .filter(|item| !verified_checklists.contains(item.checklist()))
            .map(|item| {
                let mut item = item.clone();
                item.mark_unverified();
                item
            })
            .collect();
        let count = carried.len();
        self.items.extend(carried);
        self.items
            .sort_by_key(|item| (*item.area_priority(), !*item.reopened()));
        count
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
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
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

    /// Check off named items, every open item with `*`, or the current item.
    #[instrument(level = "debug", skip(self, ids))]
    pub fn done(&mut self, ids: &[String]) -> CordialResult<Vec<ClaimChange>> {
        if ids == ["*"] {
            let open_ids = self
                .items
                .iter()
                .filter(|item| *item.state() == ItemState::Open)
                .map(|item| item.id().as_str().to_string())
                .collect::<Vec<_>>();
            if open_ids.is_empty() {
                return Ok(Vec::new());
            }
            return self.apply(&open_ids, ItemState::Done, None);
        }
        if ids.iter().any(|id| id == "*") {
            return Err(CordialError::star_must_be_alone());
        }
        self.apply(ids, ItemState::Done, None)
    }

    /// Set named items aside until the next gate run.
    #[instrument(level = "debug", skip(self, ids))]
    pub fn skip(
        &mut self,
        ids: &[String],
        reason: Option<&str>,
    ) -> CordialResult<Vec<ClaimChange>> {
        self.apply(ids, ItemState::Skipped, reason)
    }

    #[instrument(level = "debug", skip(self, ids, target))]
    fn apply(
        &mut self,
        ids: &[String],
        target: ItemState,
        reason: Option<&str>,
    ) -> CordialResult<Vec<ClaimChange>> {
        let selected = if ids.is_empty() {
            vec![self.cursor.ok_or_else(|| CordialError::no_current_item())?]
        } else {
            let mut selected = Vec::new();
            for id in ids {
                let index = self
                    .items
                    .iter()
                    .position(|item| item.id().as_str() == id)
                    .ok_or_else(|| CordialError::no_item_on_list(id))?;
                if !selected.contains(&index) {
                    selected.push(index);
                }
            }
            selected
        };
        if target == ItemState::Skipped {
            for &index in &selected {
                if *self.items[index].state() == ItemState::Done {
                    return Err(CordialError::item_already_done_cannot_skip(
                        self.items[index].id().as_str(),
                    ));
                }
            }
        }
        let cursor_was_selected = self.cursor.is_some_and(|index| selected.contains(&index));
        let mut changes = Vec::with_capacity(selected.len());
        for index in selected {
            let item = &mut self.items[index];
            let was = item.replace_state(target);
            if was != ItemState::Skipped
                && target == ItemState::Skipped
                && let Some(reason) = reason.map(str::trim).filter(|reason| !reason.is_empty())
            {
                item.add_skip_reason(reason);
            }
            changes.push(ClaimChange {
                id: item.id().as_str().to_string(),
                item_line: item.item_line().clone(),
                was,
                now: target,
            });
        }
        if cursor_was_selected {
            self.cursor = self.cursor.and_then(|index| {
                self.items
                    .iter()
                    .enumerate()
                    .skip(index + 1)
                    .find(|(_, item)| *item.state() == ItemState::Open)
                    .map(|(index, _)| index)
            });
        } else {
            self.activate();
        }
        Ok(changes)
    }
}

#[instrument(level = "trace")]
fn legacy_tiers() -> Vec<String> {
    vec!["all".to_string()]
}

#[instrument(level = "debug", skip(config))]
fn config_fingerprint(config: &CordialConfig) -> CordialResult<String> {
    let mut value = serde_json::to_value(config)?;
    sort_object_keys(&mut value);
    let bytes = serde_json::to_vec(&value)?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// HashMap iteration order must not make an unchanged config invalidate a list.
#[instrument(level = "trace", skip(value))]
fn sort_object_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.sort_keys();
            for value in map.values_mut() {
                sort_object_keys(value);
            }
        }
        serde_json::Value::Array(items) => {
            for value in items {
                sort_object_keys(value);
            }
        }
        _ => {}
    }
}

#[instrument(level = "trace")]
fn default_max_passes() -> u32 {
    3
}
