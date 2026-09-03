use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::InternalErrorComplianceId;

/// One non-compliant error-handling site.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_builder::Builder,
    derive_getters::Getters,
)]
#[builder(build_fn(error = "crate::error::CordialError"))]
pub struct InternalErrorComplianceFinding {
    /// Cargo package name.
    crate_name: String,
    /// Stable probe rule identifier.
    #[getter(copy)]
    rule_id: InternalErrorComplianceId,
    /// Qualified name or extra locator for this site.
    context: String,
    /// Source file path, usually crate-relative.
    file: PathBuf,
    /// Source line number (1-based), when known.
    #[getter(copy)]
    line: u32,
    /// Source snippet captured at the site.
    snippet: String,
    /// Foreign error type named at this site.
    foreign_error_type: Option<String>,
    /// Internal error constructor used at this site, if any.
    internal_constructor: Option<String>,
}

impl InternalErrorComplianceFinding {
    /// Start a builder for this value.
    pub fn builder() -> InternalErrorComplianceFindingBuilder {
        InternalErrorComplianceFindingBuilder::default()
    }

    pub(crate) fn strip_file_prefix(&mut self, root: &std::path::Path) {
        if let Ok(rel) = self.file.strip_prefix(root) {
            self.file = rel.to_path_buf();
        }
    }
}
