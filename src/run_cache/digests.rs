//! What each input file hashed to, remembered between runs.
//!
//! The key of a run hashes every file under every local package, and a project can keep a great
//! deal that is not source there (cordial's own rustdoc caches, a build of a verifier): reading
//! all of it was most of what a run that reused everything cost. A file whose size and
//! modification time are what they were when it was hashed is taken to hash to the same, which
//! is the assumption Git and Cargo make. A file modified within [`RACY_NANOS`] of being hashed
//! is not trusted, since a later write in the same clock tick would not change its time; it is
//! hashed again until it has stood still for longer than that.

use super::key::hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::instrument;

/// A file modified this long (two seconds) before it was hashed is not taken on trust, as the
/// clocks of some file systems count in whole seconds.
const RACY_NANOS: u128 = 2_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    len: u64,
    modified: u128,
    hashed: u128,
    digest: String,
}

/// The digests of the files a run hashed, as the last run left them.
#[derive(Debug, Default)]
pub(crate) struct DigestMemo {
    file: Option<PathBuf>,
    known: BTreeMap<String, Entry>,
}

fn nanos(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos())
}

/// The digest of a file's contents, read in pieces so that a large file is not held whole.
fn hash_contents(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

impl DigestMemo {
    /// The memo kept in `file`, empty when there is none or it cannot be read; with no file
    /// nothing is remembered.
    #[instrument(level = "debug")]
    #[hotpath::measure(impl_type = "DigestMemo")]
    pub(crate) fn open(file: Option<PathBuf>) -> Self {
        let known = file
            .as_deref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { file, known }
    }

    /// What `path` hashes to, with the entry to remember it by: from the memo when the file is
    /// as it was, from its contents otherwise. The error is that of a file that cannot be read.
    #[instrument(level = "trace", skip(self))]
    #[hotpath::measure(impl_type = "DigestMemo")]
    pub(crate) fn digest(&self, path: &Path) -> std::io::Result<(String, Remembered)> {
        let key = path.display().to_string();
        let meta = std::fs::metadata(path)?;
        let modified = meta.modified().map_or(0, nanos);
        if let Some(entry) = self.known.get(&key)
            && entry.len == meta.len()
            && entry.modified == modified
            && modified.saturating_add(RACY_NANOS) <= entry.hashed
        {
            return Ok((
                entry.digest.clone(),
                Remembered {
                    key,
                    entry: entry.clone(),
                },
            ));
        }
        let hashed = nanos(SystemTime::now());
        let digest = hash_contents(path)?;
        let entry = Entry {
            len: meta.len(),
            modified,
            hashed,
            digest: digest.clone(),
        };
        Ok((digest, Remembered { key, entry }))
    }

    /// Keep exactly the files this run hashed, so a file that is gone is forgotten. Nothing is
    /// written when that is what the memo already holds. Failing to write costs the next run
    /// the hashing and is said, not an error.
    #[instrument(level = "debug", skip(self, used))]
    #[hotpath::measure(impl_type = "DigestMemo")]
    pub(crate) fn save(&self, used: Vec<Remembered>) {
        let Some(file) = &self.file else {
            return;
        };
        let current: BTreeMap<String, Entry> = used
            .into_iter()
            .map(|remembered| (remembered.key, remembered.entry))
            .collect();
        if current == self.known {
            return;
        }
        let written = serde_json::to_vec(&current)
            .map_err(std::io::Error::other)
            .and_then(|bytes| {
                if let Some(parent) = file.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let partial = file.with_extension("partial");
                std::fs::write(&partial, bytes)?;
                std::fs::rename(&partial, file)
            });
        if let Err(error) = written {
            tracing::warn!(error = ?error, "could not remember the file digests");
        }
    }
}

/// One file's digest as the memo keeps it.
#[derive(Debug)]
pub(crate) struct Remembered {
    key: String,
    entry: Entry,
}
