//! Scan call sites for non-compliant error handling (stringify / discard typed errors).

use std::path::Path;

use crate::error::CordialResult;

use super::types::InternalErrorComplianceFinding;

use tracing::instrument;
/// Parse one source file (used by tests).
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_compliance_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    scan_compliance_rust_syntax(&syntax, file, src_root, crate_name)
}

/// Scan a pre-parsed file for internal error compliance (via unified error IR visitor).
#[instrument(level = "debug", skip(syntax, file), err(level = "warn"))]
pub fn scan_compliance_rust_syntax(
    syntax: &syn::File,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    Ok(crate::etiquettes::scan_rust_file_syntax(
        syntax,
        file,
        src_root,
        src_root,
        src_root.parent().unwrap_or(src_root),
        crate_name,
        crate::etiquettes::ErrorIrScanLayers::compliance_only()?,
    )?
    .compliance()
    .clone())
}
