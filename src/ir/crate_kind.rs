//! Cargo crate kinds, recorded once on the crate root node.

use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::{Deserialize, Serialize};
use tracing::instrument;

/// IR attribute key on the crate root node (`crate_kinds`).
///
/// The value is a JSON array of [`CrateKind::as_str`] strings. A missing key
/// means the loader had no Cargo metadata for the crate, not that the crate is
/// a plain library.
pub const ATTR_CRATE_KINDS: &str = "crate_kinds";

/// What Cargo builds a package as.
///
/// A package may be several kinds at once (a library plus a binary). Example,
/// test, bench, and build-script targets are not crate kinds and are omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CrateKind {
    /// Any library target: `lib`, `rlib`, `dylib`, `cdylib`, `staticlib`.
    Lib,
    /// A binary target.
    Bin,
    /// A `[lib] proc-macro = true` target. rustc only exports proc-macro
    /// entry points from its root, and nothing else.
    ProcMacro,
}

impl CrateKind {
    /// Stable string form of this value.
    #[instrument(level = "debug", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lib => "lib",
            Self::Bin => "bin",
            Self::ProcMacro => "proc-macro",
        }
    }

    /// Parse from the stable identifier string.
    #[instrument(level = "debug")]
    pub fn from_attr(value: &str) -> Option<Self> {
        match value {
            "lib" => Some(Self::Lib),
            "bin" => Some(Self::Bin),
            "proc-macro" => Some(Self::ProcMacro),
            _ => None,
        }
    }
}

impl Display for CrateKind {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.as_str())
    }
}
