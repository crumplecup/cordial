use std::fmt;
use std::path::Path;

use tracing::instrument;
/// Source location for findings and markers.
pub trait SourceSpan: Send + Sync {
    fn file(&self) -> &Path;
    fn line(&self) -> u32;
    fn column(&self) -> u32;
}

impl SourceSpan for () {
    fn file(&self) -> &Path {
        Path::new("")
    }

    fn line(&self) -> u32 {
        0
    }

    fn column(&self) -> u32 {
        0
    }
}

/// Concrete span used by built-in loaders and probes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FileSpan {
    pub file: std::path::PathBuf,
    pub line: u32,
    pub column: u32,
}

impl FileSpan {
    #[instrument(level = "debug", skip(file), ret)]
    pub fn new(file: impl Into<std::path::PathBuf>, line: u32, column: u32) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }
}

impl SourceSpan for FileSpan {
    #[instrument(level = "trace", skip(self))]
    fn file(&self) -> &Path {
        &self.file
    }

    #[instrument(level = "trace", skip(self))]
    fn line(&self) -> u32 {
        self.line
    }

    #[instrument(level = "trace", skip(self))]
    fn column(&self) -> u32 {
        self.column
    }
}

impl fmt::Display for FileSpan {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file.display(), self.line, self.column)
    }
}
