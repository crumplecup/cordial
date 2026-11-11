//! Which macro a macro invocation in a function body resolves to.
//!
//! A written name says little: `warn!` may be `tracing`'s, `log`'s, or a local
//! `macro_rules!`, and `use tracing::warn as w` hides the name entirely. The
//! database resolves the invocation; these facts keep the answer.

use serde::{Deserialize, Serialize};

/// One macro invocation and the definition it resolved to.
///
/// `name` is the macro's own name, not the spelling at the call site. `krate` is
/// the crate that defines it, or `None` when the invocation did not resolve (then
/// `name` is the last written segment and nothing is claimed about the macro).
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct SemanticMacroCall {
    name: String,
    krate: Option<String>,
}

impl SemanticMacroCall {
    /// Whether this resolved to one of `names` defined in crate `krate`.
    #[tracing::instrument(level = "trace", skip(self, names))]
    pub fn is(&self, krate: &str, names: &[&str]) -> bool {
        self.krate.as_deref() == Some(krate) && names.contains(&self.name.as_str())
    }
}
