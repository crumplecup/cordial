//! Reads the antipattern specimens under `tests/fixtures/quality/antipatterns`.

use miette::{IntoDiagnostic, WrapErr};
use std::fs;

pub fn fixture(name: &str) -> miette::Result<String> {
    fs::read_to_string(format!(
        "{}/tests/fixtures/quality/antipatterns/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .into_diagnostic()
    .wrap_err_with(|| format!("read fixture {name}"))
}
