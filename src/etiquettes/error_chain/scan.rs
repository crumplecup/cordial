//! syn-based scan for preserved foreign error chains.

use std::path::Path;
use walkdir::WalkDir;

use crate::error::CordialResult;
use crate::etiquettes::error_chain::ErrorChainRecord;

use tracing::instrument;
/// Scan every `src/**/*.rs` file under `crate_root`.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_chain(crate_root: &Path) -> CordialResult<Vec<ErrorChainRecord>> {
    let src_root = crate_root.join("src");
    if !src_root.is_dir() {
        return Ok(Vec::new());
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
        findings.extend(scan_rust_file(path, &src_root, crate_root)?);
    }

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.rule_id.to_string().cmp(&b.rule_id.to_string()))
    });

    Ok(findings)
}

/// Parse one source file (used by tests).
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorChainRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| crate::error::CordialError::syn_parse(file.display().to_string(), err))?;
    Ok(scan_rust_syntax(&syntax, file, src_root, crate_root))
}

/// Scan a pre-parsed file for error-chain probes (via unified error IR visitor).
#[instrument(level = "debug", skip(syntax, file))]
pub(crate) fn scan_rust_syntax(
    syntax: &syn::File,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> Vec<ErrorChainRecord> {
    let error_root = src_root.join("error");
    crate::etiquettes::scan_rust_file_syntax(
        syntax,
        file,
        src_root,
        src_root,
        &error_root,
        crate_root,
        "",
        crate::etiquettes::ErrorIrScanLayers::CHAIN_ONLY,
    )
    .chain
}

fn scan_rust_file(
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorChainRecord>> {
    let source = std::fs::read_to_string(file)?;
    scan_rust_source(&source, file, src_root, crate_root)
}
