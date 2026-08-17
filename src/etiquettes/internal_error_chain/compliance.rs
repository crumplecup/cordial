//! Scan call sites for non-compliant error handling (stringify / discard typed errors).

use std::path::Path;
use walkdir::WalkDir;

use crate::error::CordialResult;

use super::types::{InternalErrorComplianceFinding, InternalErrorComplianceReport};

/// Scan all `src/**/*.rs` for non-compliant error handling patterns.
pub fn scan_crate_internal_error_compliance(
    crate_root: &Path,
    crate_name: &str,
) -> CordialResult<InternalErrorComplianceReport> {
    let src_root = crate_root.join("src");
    if !src_root.is_dir() {
        return Ok(InternalErrorComplianceReport {
            crate_name: crate_name.to_string(),
            findings: Vec::new(),
        });
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
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.rule_id.to_string().cmp(&b.rule_id.to_string()))
    });

    for finding in &mut findings {
        if let Ok(rel) = finding.file.strip_prefix(crate_root) {
            finding.file = rel.to_path_buf();
        }
    }

    Ok(InternalErrorComplianceReport {
        crate_name: crate_name.to_string(),
        findings,
    })
}

/// Parse one source file (used by tests).
pub fn scan_compliance_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    Ok(scan_compliance_rust_syntax(
        &syntax, file, src_root, crate_name,
    ))
}

/// Scan a pre-parsed file for internal error compliance (via unified error IR visitor).
pub fn scan_compliance_rust_syntax(
    syntax: &syn::File,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
) -> Vec<InternalErrorComplianceFinding> {
    let error_root = src_root.join("error");
    crate::etiquettes::scan_rust_file_syntax(
        syntax,
        file,
        src_root,
        src_root,
        &error_root,
        src_root.parent().unwrap_or(src_root),
        crate_name,
        crate::etiquettes::ErrorIrScanLayers::COMPLIANCE_ONLY,
    )
    .compliance
}

fn scan_compliance_rust_file(
    file: &Path,
    src_root: &Path,
    crate_name: &str,
) -> CordialResult<Vec<InternalErrorComplianceFinding>> {
    let source = std::fs::read_to_string(file)?;
    scan_compliance_rust_source(&source, file, src_root, crate_name)
}
