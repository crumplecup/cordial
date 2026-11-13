//! Where run records are kept: the project's default store, under `perf/`.

use super::{PerfState, RECORD_SCHEMA, RunRecord};
use crate::{CordialError, CordialResult, StoreLayout};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::instrument;

/// Run records on disk, one JSON file each, named by [`RunRecord::id`].
#[derive(Debug, Clone, derive_getters::Getters)]
pub struct PerfStore {
    dir: PathBuf,
}

impl PerfStore {
    /// The records directory of `layout`'s store.
    #[instrument(level = "debug", skip(layout))]
    #[hotpath::measure]
    pub fn new(layout: &StoreLayout) -> Self {
        Self {
            dir: layout.perf_dir().join("records"),
        }
    }

    /// Where the raw event logs of test runs are kept, beside the records.
    #[instrument(level = "trace", skip(self))]
    pub fn raw_dir(&self) -> PathBuf {
        self.dir.with_file_name("raw")
    }

    /// Where the record named `id` is kept.
    #[instrument(level = "trace", skip(self))]
    pub fn path_of(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// Write `record` and return where it went. A record is never overwritten:
    /// two runs in the same second of the same subject are the caller's to name.
    #[instrument(level = "debug", skip(self, record), err(level = "warn"))]
    #[hotpath::measure]
    pub fn save(&self, record: &RunRecord) -> CordialResult<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path_of(&record.id());
        if path.exists() {
            return Err(CordialError::perf(path, "a record with this id exists"));
        }
        let mut staged = tempfile::NamedTempFile::new_in(&self.dir)?;
        staged.write_all(serde_json::to_string_pretty(record)?.as_bytes())?;
        staged.persist(&path).map_err(|error| error.error)?;
        Ok(path)
    }

    /// Read one record, refusing a format version this build does not know.
    #[instrument(level = "debug", skip(path), err(level = "warn"))]
    #[hotpath::measure]
    pub fn load(path: &Path) -> CordialResult<RunRecord> {
        let text = std::fs::read_to_string(path)?;
        let record: RunRecord = serde_json::from_str(&text)
            .map_err(|error| CordialError::json_parse(path.display().to_string(), error))?;
        if record.schema() != RECORD_SCHEMA {
            return Err(CordialError::perf(
                path.to_path_buf(),
                format!(
                    "format version {}, this cordial reads {RECORD_SCHEMA}",
                    record.schema()
                ),
            ));
        }
        Ok(record)
    }

    /// Every record, oldest first. A file that cannot be read is an error that
    /// names it, not a record silently left out of a comparison.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    pub fn list(&self) -> CordialResult<Vec<RunRecord>> {
        if !self.dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut records = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                records.push(Self::load(&path)?);
            }
        }
        records.sort_by_key(RunRecord::id);
        Ok(records)
    }

    /// What a baseline is kept under: the subject, and the state when it has one.
    #[instrument(level = "trace", skip(record))]
    pub fn baseline_key(record: &RunRecord) -> String {
        match record.state() {
            Some(state) => format!("{} {state}", record.subject()),
            None => record.subject().to_string(),
        }
    }

    #[instrument(level = "trace", skip(self))]
    fn baselines_path(&self) -> PathBuf {
        self.dir.with_file_name("baselines.json")
    }

    /// The marked baselines: subject (and state) to record id.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    pub fn baselines(&self) -> CordialResult<BTreeMap<String, String>> {
        let path = self.baselines_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| CordialError::json_parse(path.display().to_string(), error)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(error.into()),
        }
    }

    /// Mark `record` as the baseline for its subject and state. This is an
    /// explicit step; running a measurement never changes a baseline.
    #[instrument(level = "trace", skip(self), err(level = "warn"))]
    pub fn set_baseline(&self, record: &RunRecord) -> CordialResult<()> {
        let mut baselines = self.baselines()?;
        baselines.insert(Self::baseline_key(record), record.id());
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(
            self.baselines_path(),
            serde_json::to_string_pretty(&baselines)?,
        )?;
        Ok(())
    }

    /// The record named `query`: an id, or any prefix that names exactly one.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    pub fn find(&self, query: &str) -> CordialResult<RunRecord> {
        let mut matches: Vec<RunRecord> = self
            .list()?
            .into_iter()
            .filter(|record| record.id().starts_with(query))
            .collect();
        if let Some(exact) = matches.iter().position(|record| record.id() == query) {
            return Ok(matches.swap_remove(exact));
        }
        match matches.len() {
            1 => Ok(matches.swap_remove(0)),
            0 => Err(CordialError::perf(
                self.dir.clone(),
                format!("no record `{query}`; `cordial perf list` shows the ids"),
            )),
            count => Err(CordialError::perf(
                self.dir.clone(),
                format!("`{query}` names {count} records; give more of the id"),
            )),
        }
    }

    /// The newest record of `subject` (and `state`), if any.
    #[instrument(level = "debug", skip(self), err(level = "warn"))]
    #[hotpath::measure]
    pub fn latest(
        &self,
        subject: &str,
        state: Option<PerfState>,
    ) -> CordialResult<Option<RunRecord>> {
        Ok(self
            .list()?
            .into_iter()
            .rfind(|record| record.subject().to_string() == subject && record.state() == state))
    }
}
