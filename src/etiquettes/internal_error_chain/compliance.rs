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

/// Like [`scan_compliance_rust_source`], but `is_proc_macro_crate` tells the
/// compliance layer whether a `syn::Error` built from a foreign error at
/// this boundary is the crate's real diagnostic terminus (a `proc-macro =
/// true` library's own `expand_*` boundary) rather than a dropped chain.
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_compliance_rust_source_with_crate_kind(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    is_proc_macro_crate: bool,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    scan_compliance_rust_syntax_with_crate_kind(
        &syntax,
        file,
        src_root,
        crate_name,
        is_proc_macro_crate,
    )
}

/// Scan a pre-parsed file for internal error compliance (via unified error IR visitor).
#[instrument(level = "debug", skip(syntax, file), err(level = "warn"))]
pub fn scan_compliance_rust_syntax(
    syntax: &syn::File,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    scan_compliance_rust_syntax_with_crate_kind(syntax, file, src_root, crate_name, false)
}

/// Like [`scan_compliance_rust_syntax`], but see
/// [`scan_compliance_rust_source_with_crate_kind`] for `is_proc_macro_crate`.
#[instrument(level = "debug", skip(syntax, file), err(level = "warn"))]
pub fn scan_compliance_rust_syntax_with_crate_kind(
    syntax: &syn::File,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    is_proc_macro_crate: bool,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    Ok(crate::etiquettes::scan_rust_file_syntax(
        syntax,
        file,
        src_root,
        src_root,
        src_root.parent().unwrap_or(src_root),
        crate_name,
        crate::etiquettes::ErrorIrScanLayers::compliance_only(is_proc_macro_crate)?,
    )?
    .compliance()
    .clone())
}
