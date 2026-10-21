//! Output location for opt-in hotpath profiling.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Hotpath settings loaded from `[hotpath]` in `cordial.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct HotpathConfig {
    /// Report path; relative paths are resolved within the project's store.
    #[serde(default)]
    output_path: Option<PathBuf>,
}
