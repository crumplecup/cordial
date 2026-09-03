use std::fmt;
use std::path::Path;

use tracing::instrument;
/// Source location for findings and markers.
pub trait SourceSpan: Send + Sync {
    /// Source file this span refers to.
    fn file(&self) -> &Path;
    /// Source line (1-based); `0` when unknown.
    fn line(&self) -> u32;
    /// Source column (1-based); `0` when unknown.
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
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, derive_new::new,
)]
pub struct FileSpan {
    /// Source file path, usually crate-relative.
    #[new(into)]
    file: std::path::PathBuf,
    /// Source line number (1-based), when known.
    line: u32,
    /// Source column (1-based), when known.
    column: u32,
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
