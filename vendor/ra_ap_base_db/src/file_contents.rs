//! A file's text, which can leave memory and be read again from the file it came from.
//!
//! A library file (a dependency or the sysroot) never changes during a run, and most of them are read
//! once and not again, so its text need not stay in memory: it is dropped, and read from the disk when
//! something asks for it. A file that has no source (one edited in memory, or made up) is always
//! resident.

use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use sha2::{Digest, Sha256};
use triomphe::Arc;

/// What identifies a text: a SHA-256 of its bytes.
pub type TextDigest = [u8; 32];

fn digest_of(text: &str) -> TextDigest {
    Sha256::digest(text.as_bytes()).into()
}

/// Why a text that is on the disk could not be given: what the disk holds is not what the analysis
/// was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColdTextError {
    /// The text was dropped from memory and has nowhere to be read from.
    NoSource,
    /// The file could not be read.
    Unreadable { path: PathBuf, error: String },
    /// The file has a different length than the text had.
    LengthChanged { path: PathBuf, was: usize, is: usize },
    /// The file is as long as the text was and holds something else.
    ContentChanged { path: PathBuf, len: usize },
}

impl std::fmt::Display for ColdTextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSource => {
                write!(f, "a file's text was dropped without a source to read it from")
            }
            Self::Unreadable { path, error } => {
                write!(f, "could not read {} again: {error}", path.display())
            }
            Self::LengthChanged { path, was, is } => write!(
                f,
                "{} changed while the analysis ran: it was {was} bytes and is {is}",
                path.display()
            ),
            Self::ContentChanged { path, len } => write!(
                f,
                "{} changed while the analysis ran: it is still {len} bytes, and holds other text",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ColdTextError {}

/// A text as a saved state holds it when it is not kept in the state: where it is, and what it was.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextReference {
    pub path: PathBuf,
    pub len: usize,
    pub digest: TextDigest,
}

/// The text of a file, resident or on the disk.
#[derive(Clone)]
pub struct FileContents(Arc<Inner>);

struct Inner {
    /// The text, while it is in memory.
    resident: RwLock<Option<Arc<str>>>,
    /// Where the text can be read from again, once it is known to be on the disk as it is here.
    source: OnceLock<PathBuf>,
    /// How long the text is, to know a file that changed under the run.
    len: usize,
    /// What the text is, once asked for or restored: to know a file that changed between runs
    /// without keeping its text.
    digest: OnceLock<TextDigest>,
}

impl std::fmt::Debug for FileContents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileContents")
            .field("len", &self.0.len)
            .field("resident", &self.is_resident())
            .finish()
    }
}

impl FileContents {
    /// A text that is in memory, and stays there until it is given a source.
    pub fn resident(text: Arc<str>) -> Self {
        Self(Arc::new(Inner {
            len: text.len(),
            resident: RwLock::new(Some(text)),
            source: OnceLock::new(),
            digest: OnceLock::new(),
        }))
    }

    /// A text that is not in memory, as a saved state holds it: it is read from `reference.path`
    /// when something asks, and checked against the length and digest it had.
    pub fn cold(reference: TextReference) -> Self {
        let source = OnceLock::new();
        let digest = OnceLock::new();
        let _ = source.set(reference.path);
        let _ = digest.set(reference.digest);
        Self(Arc::new(Inner {
            len: reference.len,
            resident: RwLock::new(None),
            source,
            digest,
        }))
    }

    /// The text, read from the disk if it is not in memory.
    ///
    /// # Panics
    ///
    /// If the text was dropped and the file is not there as it was: a run does not go on with a text
    /// it cannot trust.
    pub fn get(&self) -> Arc<str> {
        self.try_get().unwrap_or_else(|error| panic!("{error}"))
    }

    /// The text, read from the disk if it is not in memory, or why it cannot be given.
    pub fn try_get(&self) -> Result<Arc<str>, ColdTextError> {
        if let Ok(resident) = self.0.resident.read()
            && let Some(text) = resident.as_ref()
        {
            return Ok(Arc::clone(text));
        }
        let Some(path) = self.0.source.get() else {
            return Err(ColdTextError::NoSource);
        };
        let text = std::fs::read_to_string(path).map_err(|error| ColdTextError::Unreadable {
            path: path.clone(),
            error: error.to_string(),
        })?;
        if text.len() != self.0.len {
            return Err(ColdTextError::LengthChanged {
                path: path.clone(),
                was: self.0.len,
                is: text.len(),
            });
        }
        if let Some(digest) = self.0.digest.get()
            && digest_of(&text) != *digest
        {
            return Err(ColdTextError::ContentChanged { path: path.clone(), len: self.0.len });
        }
        let text: Arc<str> = Arc::from(text);
        if let Ok(mut resident) = self.0.resident.write() {
            *resident = Some(Arc::clone(&text));
        }
        Ok(text)
    }

    /// Whether the text is `other`, without reading it from the disk when its digest is known: what a
    /// load that has just read the file asks of the state it restored, to leave it alone when it is
    /// unchanged and write it when it is not.
    pub fn is(&self, other: &str) -> bool {
        if other.len() != self.0.len {
            return false;
        }
        if let Ok(resident) = self.0.resident.read()
            && let Some(text) = resident.as_ref()
        {
            return **text == *other;
        }
        match self.0.digest.get() {
            Some(digest) => digest_of(other) == *digest,
            None => self.try_get().is_ok_and(|text| *text == *other),
        }
    }

    /// Say that the text can be read from `path` as it is here, without dropping it from memory, for
    /// a state that saves it as a reference. Returns whether it was attached: it is not when the file
    /// is not on the disk with this length, which is a text made up in memory or a file that is not
    /// as the analysis read it.
    pub fn attach_source(&self, path: &Path) -> bool {
        if self.0.source.get().is_some() {
            return true;
        }
        let on_disk = std::fs::metadata(path).is_ok_and(|metadata| {
            metadata.is_file() && usize::try_from(metadata.len()).is_ok_and(|len| len == self.0.len)
        });
        on_disk && self.0.source.set(path.to_path_buf()).is_ok()
    }

    /// What a state holds of this text instead of the text, when it has a source: the digest is
    /// computed from the text once, while it is in memory.
    pub fn reference(&self) -> Option<TextReference> {
        let path = self.0.source.get()?.clone();
        let digest = match self.0.digest.get() {
            Some(digest) => *digest,
            None => {
                let resident = self.0.resident.read().ok()?;
                let digest = digest_of(resident.as_ref()?);
                *self.0.digest.get_or_init(|| digest)
            }
        };
        Some(TextReference { path, len: self.0.len, digest })
    }

    /// Whether the text is in memory.
    pub fn is_resident(&self) -> bool {
        self.0.resident.read().is_ok_and(|resident| resident.is_some())
    }

    /// How long the text is.
    pub fn len(&self) -> usize {
        self.0.len
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        self.0.len == 0
    }

    /// Say where the text can be read from again, and drop it from memory. A text that already has a
    /// source keeps that one. Returns how many bytes left memory (none if it was not in memory).
    pub fn make_cold(&self, source: PathBuf) -> usize {
        let _ = self.0.source.set(source);
        let _ = self.reference();
        self.drop_resident()
    }

    /// Drop the text from memory if it has a source to read it from. Returns how many bytes left.
    pub fn drop_resident(&self) -> usize {
        if self.0.source.get().is_none() {
            return 0;
        }
        match self.0.resident.write() {
            Ok(mut resident) => resident.take().map_or(0, |text| text.len()),
            Err(_) => 0,
        }
    }
}
