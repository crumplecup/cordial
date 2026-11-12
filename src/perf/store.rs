//! Where run records are kept: the project's default store, under `perf/`.

use super::{RECORD_SCHEMA, RunRecord};
use crate::{CordialError, CordialResult, StoreLayout};
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
}
