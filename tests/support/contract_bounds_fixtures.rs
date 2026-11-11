//! Where the antipattern specimens live.
#![cfg(feature = "antipatterns")]

use std::path::PathBuf;

pub fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/quality/antipatterns")
}
