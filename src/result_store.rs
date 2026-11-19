//! The results that processes of one run hand to each other, in a `redb` file on the disk.
//!
//! A run that works in processes that come and go (see `isolation`) cannot hand the next one its
//! database: what the database holds is tied to numbers that mean something only in the process
//! that made them. What it can hand on is what means the same in every process: a result, by the
//! text that names what it is the result of. A process writes each result as it computes it, so that
//! whichever process ends the run, the results are there; the one that comes after reads the names
//! of what is done, and does not do it again.
//!
//! One process has the file open at a time (`redb` says so): the processes of a run follow one
//! another. A write is gathered with others and made a few megabytes at a time, and each is
//! committed durably, because the process that wrote it is about to end.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use redb::{Database, ReadableDatabase, ReadableTableMetadata, TableDefinition};
use tracing::instrument;

/// `(kind, name)` to the encoded result.
const RESULTS: TableDefinition<(&str, &str), &[u8]> = TableDefinition::new("results");

/// Set by the driver of a run in processes to the file the results go in. A process that finds it
/// writes what it computes there, and reads what an earlier process did.
pub const RESULT_STORE_VARIABLE: &str = "CORDIAL_RESULT_STORE";

/// The kind of the results the evidence of a function is: by the function's description.
pub const EVIDENCE: &str = "evidence";

/// The store this process writes to and reads from, if its run has one.
static CURRENT: RwLock<Option<Arc<ResultStore>>> = RwLock::new(None);

/// Make `store` the one this process writes to and reads from (or none).
pub fn install(store: Option<Arc<ResultStore>>) {
    if let Ok(mut current) = CURRENT.write() {
        *current = store;
    }
}

/// The store this process writes to and reads from, if its run has one.
#[must_use]
pub fn current() -> Option<Arc<ResultStore>> {
    CURRENT.read().ok().and_then(|current| current.clone())
}

/// Whether this run keeps its results in a store, and so in processes that come and go.
#[must_use]
pub fn active() -> bool {
    current().is_some()
}

/// Open the store the driver of this run named, and make it this process's. Says why when it cannot:
/// a process that cannot keep its results cannot be one of a run that hands them on.
///
/// # Errors
///
/// When the driver named a file and it cannot be opened.
#[cfg(feature = "cli")]
#[instrument(level = "debug", err(level = "warn"))]
pub fn install_from_environment() -> std::io::Result<()> {
    let Some(path) = std::env::var_os(RESULT_STORE_VARIABLE) else {
        return Ok(());
    };
    install(Some(Arc::new(ResultStore::open(Path::new(&path))?)));
    Ok(())
}

/// Write what the installed store has gathered. A process that ends without returning does it first.
pub fn flush_current() {
    if let Some(store) = current() {
        store.flush();
    }
}

/// Writes what the installed store has gathered when it goes, so that a process that ends by
/// returning leaves its results as one that exits does.
#[cfg(feature = "cli")]
#[derive(Debug)]
pub struct FlushOnDrop;

#[cfg(feature = "cli")]
impl Drop for FlushOnDrop {
    fn drop(&mut self) {
        flush_current();
    }
}

/// How many bytes are gathered before they are written.
const WRITE_EVERY_BYTES: usize = 4 << 20;

#[derive(Default)]
struct Gathered {
    results: HashMap<(String, String), Vec<u8>>,
    bytes: usize,
}

/// The file the results are in.
pub struct ResultStore {
    database: Database,
    path: PathBuf,
    gathered: Mutex<Gathered>,
}

impl std::fmt::Debug for ResultStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResultStore")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl ResultStore {
    /// Open the file at `path`, making it (and its directory) if it is not there.
    ///
    /// # Errors
    ///
    /// When the file cannot be made or opened, or another process has it open.
    #[instrument(level = "debug", err(level = "warn"))]
    pub fn open(path: &Path) -> std::io::Result<Self> {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)?;
        }
        let database = Database::create(path).map_err(std::io::Error::other)?;
        Ok(Self {
            database,
            path: path.to_owned(),
            gathered: Mutex::new(Gathered::default()),
        })
    }

    /// Where the file is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keep `bytes` as the result named `name` of its `kind`, replacing what was there.
    pub fn put(&self, kind: &str, name: &str, bytes: &[u8]) {
        let due = {
            let Ok(mut gathered) = self.gathered.lock() else {
                return;
            };
            gathered.bytes += bytes.len() + name.len();
            gathered
                .results
                .insert((kind.to_owned(), name.to_owned()), bytes.to_vec());
            (gathered.bytes >= WRITE_EVERY_BYTES).then(|| std::mem::take(&mut *gathered))
        };
        if let Some(due) = due {
            self.write(due.results);
        }
    }

    /// The result named `name` of its `kind`, if there is one: what was put and not yet written
    /// counts, as what was written does.
    #[must_use]
    pub fn get(&self, kind: &str, name: &str) -> Option<Vec<u8>> {
        if let Ok(gathered) = self.gathered.lock()
            && let Some(bytes) = gathered.results.get(&(kind.to_owned(), name.to_owned()))
        {
            return Some(bytes.clone());
        }
        let transaction = self.database.begin_read().ok()?;
        let table = transaction.open_table(RESULTS).ok()?;
        let found = table.get((kind, name)).ok()??;
        Some(found.value().to_vec())
    }

    /// The names of every result of `kind`, those not yet written included: what a process that
    /// takes up the run asks first, to leave what is done.
    #[must_use]
    pub fn names(&self, kind: &str) -> HashSet<String> {
        let mut names: HashSet<String> = HashSet::new();
        if let Ok(transaction) = self.database.begin_read()
            && let Ok(table) = transaction.open_table(RESULTS)
            && let Ok(range) = table.range((kind, "")..)
        {
            for entry in range.flatten() {
                let (found, name) = entry.0.value();
                if found != kind {
                    break;
                }
                names.insert(name.to_owned());
            }
        }
        if let Ok(gathered) = self.gathered.lock() {
            names.extend(
                gathered
                    .results
                    .keys()
                    .filter(|(found, _)| found == kind)
                    .map(|(_, name)| name.clone()),
            );
        }
        names
    }

    /// How many results the file has (those not yet written included), of every kind.
    #[must_use]
    pub fn count(&self) -> u64 {
        let written = self
            .database
            .begin_read()
            .ok()
            .and_then(|transaction| transaction.open_table(RESULTS).ok())
            .and_then(|table| table.len().ok())
            .unwrap_or(0);
        let gathered = self
            .gathered
            .lock()
            .map_or(0, |gathered| gathered.results.len());
        written + u64::try_from(gathered).unwrap_or(0)
    }

    /// Write what is gathered. The process that is about to end says so first.
    pub fn flush(&self) {
        let due = self
            .gathered
            .lock()
            .map(|mut gathered| std::mem::take(&mut *gathered))
            .unwrap_or_default();
        self.write(due.results);
    }

    /// How many bytes the file takes on the disk once what is gathered is written: how far the run
    /// has got, to a process that cannot count what it did not do.
    #[must_use]
    pub fn bytes_on_disk(&self) -> u64 {
        self.flush();
        std::fs::metadata(&self.path).map_or(0, |metadata| metadata.len())
    }

    fn write(&self, results: HashMap<(String, String), Vec<u8>>) {
        if results.is_empty() {
            return;
        }
        // In key order, which is the order a table is kept in.
        let ordered: BTreeMap<_, _> = results.into_iter().collect();
        let written = (|| -> Result<(), redb::Error> {
            let transaction = self.database.begin_write()?;
            {
                let mut table = transaction.open_table(RESULTS)?;
                for ((kind, name), bytes) in &ordered {
                    table.insert((kind.as_str(), name.as_str()), bytes.as_slice())?;
                }
            }
            transaction.commit()?;
            Ok(())
        })();
        if let Err(error) = written {
            tracing::error!(
                target: "cordial::results",
                error = ?error,
                results = ordered.len(),
                path = %self.path.display(),
                "could not write results"
            );
        }
    }
}

impl Drop for ResultStore {
    fn drop(&mut self) {
        // What a process put is what the next must find, so the end of this one writes it.
        self.flush();
    }
}
