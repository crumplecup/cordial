//! Scan call sites for non-compliant error handling (stringify / discard typed errors).

use std::path::Path;
use walkdir::WalkDir;

use crate::error::CordialResult;

use super::types::{InternalErrorComplianceFinding, InternalErrorComplianceReport};

use tracing::instrument;
/// Scan all `src/**/*.rs` for non-compliant error handling patterns.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_internal_error_compliance(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<InternalErrorComplianceReport> {
    let src_root = crate_root.join("src");
    if !src_root.is_dir() {
        return Ok(InternalErrorComplianceReport::new(
            crate_name.to_string(),
            Vec::new(),
        ));
    }

    let mut findings = Vec::new();
    for entry in WalkDir::new(&src_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let mut file_findings = scan_compliance_rust_file(path, &src_root, crate_name)?;
        findings.append(&mut file_findings);
    }

    findings.sort_by(|a, b| {
        a.file()
            .cmp(b.file())
            .then(a.line().cmp(&b.line()))
            .then(a.rule_id().to_string().cmp(&b.rule_id().to_string()))
    });

    let findings = findings
        .into_iter()
        .map(|finding| relativize_compliance_finding(finding, crate_root))
        .collect::<CordialResult<Vec<_>>>()?;

    Ok(InternalErrorComplianceReport::new(
        crate_name.to_string(),
        findings,
    ))
}

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
        crate::etiquettes::ErrorIrScanLayers::COMPLIANCE_ONLY,
    )?
    .compliance()
    .clone())
}

#[instrument(level = "debug", skip(finding))]
fn relativize_compliance_finding(
    finding: InternalErrorComplianceFinding,
    crate_root: &Path,
) -> CordialResult<InternalErrorComplianceFinding> {
    let file = finding
        .file()
        .strip_prefix(crate_root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| finding.file().clone());
    InternalErrorComplianceFinding::builder()
        .crate_name(finding.crate_name().clone())
        .rule_id(finding.rule_id())
        .context(finding.context().clone())
        .file(file)
        .line(finding.line())
        .snippet(finding.snippet().clone())
        .foreign_error_type(finding.foreign_error_type().clone())
        .internal_constructor(finding.internal_constructor().clone())
        .build()
}

#[instrument(level = "debug", skip(file), err(level = "warn"))]
fn scan_compliance_rust_file(
    file: &Path,
    src_root: &Path,
    crate_name: &str,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    let source = std::fs::read_to_string(file)?;
    scan_compliance_rust_source(&source, file, src_root, crate_name)
}
