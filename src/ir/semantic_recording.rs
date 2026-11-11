//! What a function's formatting and recording obligations are, proven by the
//! database's solver in the function's own environment: the error's `Display` and
//! `Debug`, each parameter's `Debug`, and whether a declaration is itself a
//! formatting method.

use serde::{Deserialize, Serialize};

/// A formatting obligation checked in the function's own generic environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticCapability {
    /// The semantic solver established the actual core formatting trait.
    Proven,
    /// Resolution succeeded, but the solver did not establish the obligation.
    /// This is not proof that no implementation can exist.
    NotProven,
    /// The type or the core trait could not be resolved.
    Unavailable,
}

/// Recording capabilities of the actual Result error, independent of success.
/// These facts do not depend on the bounded `SemanticType` representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SemanticResultRecording {
    /// The resolved output is not core Result, even if its name ends in Result.
    NotResult,
    /// Instantiated error capabilities, including references and generic bounds.
    Result {
        /// Suitability for `instrument(err)`.
        display: SemanticCapability,
        /// Suitability for `instrument(err(Debug))`.
        debug: SemanticCapability,
    },
    /// Missing evidence must never become a non-fallible contract.
    #[default]
    Unavailable,
}

/// Formatting methods need a recursion-aware recording policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SemanticFormattingRole {
    /// The declaration is known not to belong to a core formatting trait.
    Ordinary,
    /// The actual core Display trait, not a similarly named local trait.
    Display,
    /// The actual core Debug trait.
    Debug,
    /// Missing trait or implementation evidence cannot prove recursion safety.
    #[default]
    Unavailable,
}

/// One binding introduced by a function's parameters and whether `Debug` is
/// proven for its type in the function's own environment, which is what lets
/// `#[instrument]` record it.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct SemanticParameter {
    name: String,
    debug: SemanticCapability,
}
