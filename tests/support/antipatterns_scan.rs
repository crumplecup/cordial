//! Scans one antipattern specimen as a source file.

use cordial::scan_antipatterns_rust_source;
use miette::IntoDiagnostic;
use std::path::Path;

use crate::antipatterns_fixture::fixture;

pub fn scan_fixture(name: &str) -> miette::Result<Vec<cordial::AntipatternSiteRecord>> {
    let src_root = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/quality/antipatterns"
    ));
    let file = src_root.join(name);
    scan_antipatterns_rust_source(&fixture(name)?, &file, src_root, src_root).into_diagnostic()
}
