use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::InternalErrorComplianceId;

/// One non-compliant error-handling site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalErrorComplianceFinding {
    /// Cargo package name.
    pub crate_name: String,
    /// Stable probe rule identifier.
    pub rule_id: InternalErrorComplianceId,
    /// Qualified name or extra locator for this site.
    pub context: String,
    /// Source file path, usually crate-relative.
    pub file: PathBuf,
    /// Source line number (1-based), when known.
    pub line: u32,
    /// Source snippet captured at the site.
    pub snippet: String,
    /// Foreign error type named at this site.
    pub foreign_error_type: Option<String>,
    /// Internal error constructor used at this site, if any.
    pub internal_constructor: Option<String>,
}
