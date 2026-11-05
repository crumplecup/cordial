//! syn-based scan for preserved foreign error chains.

use crate::loader::SourceLoadView;
use std::path::Path;

use crate::error::CordialResult;
use crate::etiquettes::error_chain::ErrorChainRecord;

use tracing::instrument;
/// Scan every `src/**/*.rs` file under `crate_root`.
#[instrument(level = "debug", err(level = "warn"))]
pub fn scan_crate_error_chain(crate_root: &Path) -> CordialResult<Vec<ErrorChainRecord>> {
    let source = SourceLoadView::for_standalone_scan();
    Ok(crate::ir::SourceErrorChains::collect(&source, crate_root)?.records())
}

/// Parse one source file (used by tests).
#[instrument(level = "debug", skip(source, file), err(level = "warn"))]
pub fn scan_rust_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorChainRecord>> {
    let syntax = SourceLoadView::parse_fixture(file, source)?;
    scan_rust_syntax(&syntax, src_root, crate_root)
}

/// Scan a pre-parsed file for error-chain probes (via unified error IR visitor).
#[instrument(level = "debug", skip(syntax), err(level = "warn"))]
pub(crate) fn scan_rust_syntax(
    syntax: &crate::SharedParsedFile,
    src_root: &Path,
    crate_root: &Path,
) -> CordialResult<Vec<ErrorChainRecord>> {
    Ok(crate::etiquettes::scan_rust_file_syntax(
        syntax.syntax(),
        syntax.path(),
        src_root,
        src_root,
        crate_root,
        "",
        crate::etiquettes::ErrorIrScanLayers::chain_only()?,
    )?
    .chain()
    .clone())
}
