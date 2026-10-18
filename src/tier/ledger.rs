//! Last completed run of each etiquette in the project store.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{
    CordialConfig, CordialError, CordialResult, Disposition, Etiquette, Finding, StoreLayout,
};

use super::{HookTiming, HookTimings, assigned_tier};

/// Last completed run of one etiquette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct EtiquetteLedgerEntry {
    tier: String,
    last_run_unix_secs: u64,
    open_count: usize,
    wall_time_nanos: u128,
}

/// Per-etiquette history for report coverage and timing comparison.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct GateLedger {
    #[serde(default)]
    entries: BTreeMap<String, EtiquetteLedgerEntry>,
    #[serde(default)]
    area_open_counts: BTreeMap<String, usize>,
    #[serde(default)]
    last_run_timings: Vec<HookTiming>,
    #[serde(default)]
    last_run_wall_nanos: u128,
}

impl GateLedger {
    /// Save the latest run's phase, hook, and pipeline timings for inspection.
    #[instrument(level = "debug", skip(self, timings))]
    pub fn record_last_run_timings(&mut self, timings: Vec<HookTiming>, wall: std::time::Duration) {
        self.last_run_timings = timings;
        self.last_run_wall_nanos = wall.as_nanos();
    }
    /// Save exact area counts only when every contributing etiquette ran.
    #[cfg(feature = "quality")]
    #[instrument(level = "debug", skip(self, report))]
    pub(crate) fn record_quality_areas(
        &mut self,
        report: &crate::QualityReport,
        active_ids: &[&str],
    ) {
        for area in report.areas() {
            let owners = crate::reporter::quality_area_owners(area.title());
            if !owners.is_empty() && owners.iter().all(|id| active_ids.contains(id)) {
                self.area_open_counts
                    .insert(area.title().clone(), area.open_items());
            }
        }
    }
    /// Read the previous ledger, or start an empty one.
    #[instrument(level = "debug", skip(store), err(level = "warn"))]
    pub fn read(store: &StoreLayout) -> CordialResult<Self> {
        let path = store.root().join("ledger.json");
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| CordialError::json_parse(path.display().to_string(), error)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// Update only etiquettes that ran, preserving entries for unselected ones.
    #[instrument(
        level = "debug",
        skip(self, etiquettes, findings, timings, config),
        err(level = "warn")
    )]
    pub fn record_run(
        &mut self,
        etiquettes: &[std::sync::Arc<dyn Etiquette>],
        findings: &[Box<dyn Finding>],
        timings: &HookTimings,
        config: &CordialConfig,
    ) -> CordialResult<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| CordialError::invariant(error.to_string()))?
            .as_secs();
        for etiquette in etiquettes {
            let open_count = findings
                .iter()
                .filter(|finding| {
                    finding.disposition() == Disposition::Open
                        && finding.rule().category() == etiquette.id()
                })
                .count();
            let wall_time_nanos = etiquette
                .loaders()
                .iter()
                .map(|hook| timings.nanos_for("load", hook.id()))
                .chain(
                    etiquette
                        .enrichers()
                        .iter()
                        .map(|hook| timings.nanos_for("enrich", hook.id())),
                )
                .chain(
                    etiquette
                        .probes()
                        .iter()
                        .map(|hook| timings.nanos_for("probe", hook.id())),
                )
                .chain(
                    etiquette
                        .assessors()
                        .iter()
                        .map(|hook| timings.nanos_for("assess", hook.id())),
                )
                .chain(
                    etiquette
                        .workspace_assessors()
                        .iter()
                        .map(|hook| timings.nanos_for("workspace_assess", hook.id())),
                )
                .chain(
                    etiquette
                        .reporters()
                        .iter()
                        .map(|hook| timings.nanos_for("report", hook.id())),
                )
                .sum();
            self.entries.insert(
                etiquette.id().to_string(),
                EtiquetteLedgerEntry {
                    tier: assigned_tier(etiquette.as_ref(), config)
                        .as_str()
                        .to_string(),
                    last_run_unix_secs: now,
                    open_count,
                    wall_time_nanos,
                },
            );
        }
        Ok(())
    }

    /// Atomically replace the ledger after a successful run.
    #[instrument(level = "debug", skip(self, store), err(level = "warn"))]
    pub fn write(&self, store: &StoreLayout) -> CordialResult<()> {
        fs::create_dir_all(store.root())?;
        let path = store.root().join("ledger.json");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| CordialError::invariant(error.to_string()))?
            .as_nanos();
        let temporary = store
            .root()
            .join(format!("ledger.json.tmp-{}-{stamp}", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| -> CordialResult<()> {
            serde_json::to_writer_pretty(&mut file, self)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&temporary, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}
