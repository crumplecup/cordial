//! Shared snapshot validation and rendering for isolated function operations.

use crate::error::{CordialError, CordialResult};
use crate::ir::{NodeId, SourceByteRange};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// A located replacement in the collected source snapshot.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct FunctionEdit {
    range: SourceByteRange,
    replacement: String,
}

/// Located edits for one isolated function operation; component obligations remain.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
#[new(visibility = "pub(super)")]
pub struct FunctionEditPlan {
    #[getter(copy)]
    function: NodeId,
    path: PathBuf,
    #[getter(copy)]
    source_digest: [u8; 32],
    edits: Vec<FunctionEdit>,
}

impl FunctionEditPlan {
    /// Render a deterministic preview against the exact collected bytes.
    ///
    /// Reads text only for located edits; does not parse or rediscover evidence.
    /// No source files or worklist claims are changed by this method.
    #[tracing::instrument(level = "debug", skip(self, source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "FunctionEditPlan")]
    pub fn preview(&self, source: &str) -> CordialResult<String> {
        let digest: [u8; 32] = Sha256::digest(source.as_bytes()).into();
        if digest != self.source_digest {
            return Err(CordialError::stale_source_snapshot(&self.path));
        }
        // Validate every range before constructing any output, including UTF-8
        // boundaries. Malformed persisted facts must never panic in replace_range.
        let mut previous_end = 0;
        for edit in &self.edits {
            let range = edit.range.start()..edit.range.end();
            if range.start < previous_end || source.get(range.clone()).is_none() {
                return Err(std::io::Error::from(std::io::ErrorKind::InvalidData).into());
            }
            previous_end = range.end;
        }
        let mut output = source.to_owned();
        for edit in self.edits.iter().rev() {
            output.replace_range(edit.range.start()..edit.range.end(), &edit.replacement);
        }
        Ok(output)
    }
}
