//! Where the values of memos go when the run is short of memory: a `redb` file beside the saved
//! state, in the run cache, removed when the run ends.
//!
//! Salsa writes a persisted memo's value here and drops it from memory, and reads it back when a
//! query asks for the memo (see `salsa::set_spill_store`). The writes are gathered and made a few
//! megabytes at a time, because one transaction for each of many thousand values would be the cost
//! of the spill; a value not written yet is read from what is gathered.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use redb::{Database, ReadableDatabase, TableDefinition};
use salsa::{SpillKey, SpillStore};
use tracing::instrument;

use crate::{CordialError, CordialResult};

/// `(database, query, key)` to the encoded value.
const MEMOS: TableDefinition<(u32, u32, u64), &[u8]> = TableDefinition::new("memos");

/// How many bytes are gathered before they are written.
const WRITE_EVERY_BYTES: usize = 8 << 20;

#[derive(Default)]
struct Gathered {
    values: HashMap<SpillKey, Vec<u8>>,
    bytes: usize,
}

/// A run's spilled values: the file, and the store Salsa is told of, which is cleared when the run's
/// database goes.
#[derive(Debug)]
pub(super) struct Spilling {
    file: std::sync::Arc<SpillFile>,
}

impl Spilling {
    /// Open the file at `path` and make it the store Salsa spills to.
    pub(super) fn begin(path: &Path) -> CordialResult<Self> {
        let file = std::sync::Arc::new(SpillFile::create(path)?);
        salsa::set_spill_store(Some(file.clone()));
        Ok(Self { file })
    }
}

impl Drop for Spilling {
    fn drop(&mut self) {
        salsa::set_spill_store(None);
        let _ = &self.file;
    }
}

/// The file the spilled values are in.
pub(super) struct SpillFile {
    database: Database,
    path: PathBuf,
    gathered: Mutex<Gathered>,
}

impl std::fmt::Debug for SpillFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SpillFile")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl SpillFile {
    /// Make the file `path` (its directory too), after removing the files of runs that are over.
    #[instrument(level = "debug", err(level = "warn"))]
    pub(super) fn create(path: &Path) -> CordialResult<Self> {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)?;
            sweep_abandoned(directory, path);
        }
        let database = Database::create(path).map_err(|error| storage(&error))?;
        Ok(Self {
            database,
            path: path.to_owned(),
            gathered: Mutex::new(Gathered::default()),
        })
    }

    fn write(&self, values: HashMap<SpillKey, Vec<u8>>) {
        if values.is_empty() {
            return;
        }
        let written = (|| -> Result<(), redb::Error> {
            let mut transaction = self.database.begin_write()?;
            // Nothing here must survive the run, so nothing needs to reach the disk before it
            // is acknowledged.
            transaction.set_durability(redb::Durability::None)?;
            {
                let mut table = transaction.open_table(MEMOS)?;
                for (key, bytes) in &values {
                    table.insert((key.storage, key.ingredient, key.id), bytes.as_slice())?;
                }
            }
            transaction.commit()?;
            Ok(())
        })();
        if let Err(error) = written {
            // The values are gone from memory; a read of one will say it is not in the store.
            tracing::error!(error = ?error, values = values.len(), "could not write spilled values");
        }
    }

    fn read(&self, key: SpillKey) -> Option<Vec<u8>> {
        let transaction = self.database.begin_read().ok()?;
        let table = transaction.open_table(MEMOS).ok()?;
        let value = table.get((key.storage, key.ingredient, key.id)).ok()??;
        Some(value.value().to_vec())
    }
}

impl SpillStore for SpillFile {
    fn put(&self, key: SpillKey, bytes: &[u8]) {
        let due = {
            let Ok(mut gathered) = self.gathered.lock() else {
                return;
            };
            gathered.bytes += bytes.len();
            gathered.values.insert(key, bytes.to_vec());
            (gathered.bytes >= WRITE_EVERY_BYTES).then(|| std::mem::take(&mut *gathered))
        };
        if let Some(due) = due {
            self.write(due.values);
        }
    }

    fn get(&self, key: SpillKey) -> Option<Vec<u8>> {
        if let Ok(gathered) = self.gathered.lock()
            && let Some(bytes) = gathered.values.get(&key)
        {
            return Some(bytes.clone());
        }
        self.read(key)
    }

    fn flush(&self) {
        let due = self
            .gathered
            .lock()
            .map(|mut gathered| std::mem::take(&mut *gathered))
            .unwrap_or_default();
        self.write(due.values);
    }
}

impl Drop for SpillFile {
    fn drop(&mut self) {
        // The values are of this run's memos and mean nothing to another.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Remove the spill files in `directory` that belong to runs that are no longer there (named for
/// their process), so that a run that was killed does not leave its values on the disk.
fn sweep_abandoned(directory: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let alive = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse::<u32>().ok())
            .is_none_or(|process| Path::new("/proc").join(process.to_string()).exists());
        if path != keep && !alive {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn storage(error: &redb::DatabaseError) -> CordialError {
    CordialError::from(std::io::Error::other(error.to_string()))
}
