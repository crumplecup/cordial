//! Snapshot identity for non-syntax inputs retained by the shared loader.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Cargo manifest bytes observed when discovering the target.
pub const ATTR_CARGO_MANIFEST_SNAPSHOT: &str = "cargo_manifest_snapshot";

/// Exact bytes on which a collected input depended.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct FileSnapshot {
    path: PathBuf,
    #[getter(copy)]
    digest: [u8; 32],
}
