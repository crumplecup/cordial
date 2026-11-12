//! Which function a call resolved to, by its definition and not by how it was written.
//!
//! `init()` may be a subscriber's installation or something unrelated; `.init()` on
//! a builder resolves through a trait the call site never names. The database
//! resolves the call; these facts keep what it resolved to.

use serde::{Deserialize, Serialize};

/// The definition a call resolved to.
///
/// `path` is the canonical path of the definition: its crate, its modules, the
/// trait or type it belongs to, and its own name, for example
/// `tracing_subscriber::util::SubscriberInitExt::init`. A re-export does not
/// change it, so a rule compares against where a function is defined.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct SemanticCallee {
    name: String,
    krate: Option<String>,
    path: String,
}

impl SemanticCallee {
    /// Whether this is one of the standard library's `names`, a method of one of
    /// the types `owners`.
    #[tracing::instrument(level = "trace", skip(self, owners, names))]
    pub fn is_std_method(&self, owners: &[&str], names: &[&str]) -> bool {
        self.is(&["core", "std"], names)
            && self
                .path
                .rsplit("::")
                .nth(1)
                .is_some_and(|owner| owners.contains(&owner))
    }

    /// Whether this is one of `names`, defined in one of the crates `krates`.
    #[tracing::instrument(level = "trace", skip(self, krates, names))]
    pub fn is(&self, krates: &[&str], names: &[&str]) -> bool {
        self.krate
            .as_deref()
            .is_some_and(|krate| krates.contains(&krate))
            && names.contains(&self.name.as_str())
    }
}
