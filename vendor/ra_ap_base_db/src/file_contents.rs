//! A file's text, which can leave memory and be read again from the file it came from.
//!
//! A library file (a dependency or the sysroot) never changes during a run, and most of them are read
//! once and not again, so its text need not stay in memory: it is dropped, and read from the disk when
//! something asks for it. A file that has no source (one edited in memory, or made up) is always
//! resident.

use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use triomphe::Arc;

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
        }))
    }

    /// The text, read from the disk if it is not in memory.
    ///
    /// # Panics
    ///
    /// If the text was dropped and the file is not there as it was: a run does not go on with a text
    /// it cannot trust.
    pub fn get(&self) -> Arc<str> {
        if let Ok(resident) = self.0.resident.read()
            && let Some(text) = resident.as_ref()
        {
            return Arc::clone(text);
        }
        let Some(path) = self.0.source.get() else {
            panic!("a file's text was dropped without a source to read it from");
        };
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("could not read {} again: {error}", path.display()));
        assert!(
            text.len() == self.0.len,
            "{} changed while the analysis ran: it was {} bytes and is {}",
            path.display(),
            self.0.len,
            text.len()
        );
        let text: Arc<str> = Arc::from(text);
        if let Ok(mut resident) = self.0.resident.write() {
            *resident = Some(Arc::clone(&text));
        }
        text
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
