//! The resolved identity of a type.

use std::fmt::{Display, Formatter, Result as FmtResult};

use tracing::instrument;

/// A resolved type: a canonical head path and its resolved arguments.
///
/// Two spellings of one type resolve to equal keys. A head outside the
/// resolver's crate allowlist is *opaque*: its path as spelled, never
/// expanded, still comparable by identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_new::new, derive_getters::Getters)]
pub struct TypeKey {
    /// Canonical path of the type constructor, e.g. `chrono::DateTime`.
    head: String,
    /// Resolved generic arguments, in order; empty for a non-generic type.
    args: Vec<TypeKey>,
}

impl TypeKey {
    /// A key with no generic arguments.
    #[instrument(level = "debug", skip(head))]
    pub fn plain(head: impl Into<String> + std::fmt::Debug) -> Self {
        Self::new(head.into(), Vec::new())
    }

    /// Number of nodes in this key (the head plus all nested arguments).
    #[instrument(level = "trace", skip(self))]
    pub fn node_count(&self) -> usize {
        1 + self.args.iter().map(Self::node_count).sum::<usize>()
    }
}

impl Display for TypeKey {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.head)?;
        if self.args.is_empty() {
            return Ok(());
        }
        f.write_str("<")?;
        for (index, arg) in self.args.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{arg}")?;
        }
        f.write_str(">")
    }
}
