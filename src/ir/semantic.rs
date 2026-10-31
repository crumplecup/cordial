//! Snapshot-local semantic evidence projected from the shared source database.

use serde::{Deserialize, Serialize};

/// Input identity of one semantic graph projection.
pub const ATTR_SEMANTIC_SNAPSHOT: &str = "semantic_snapshot";

/// Source revision and exact input bytes behind a semantic projection.
/// This records evidence provenance; it is not a source-write capability.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct SemanticSnapshot {
    revision: u64,
    sources: Vec<super::FileSnapshot>,
}

/// Function evidence produced by the canonical semantic database.
pub const ATTR_SEMANTIC_FUNCTION: &str = "semantic_function";

/// An interned definition identity, meaningful only inside its analysis session.
/// Labels and printed type paths are never used to decide identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, derive_new::new)]
#[new(visibility = "pub(crate)")]
pub struct SemanticItemId(u64);

/// A deliberately bounded type summary; unsupported types remain explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticType {
    /// A resolved algebraic type. Generic arguments are not yet projected.
    Adt {
        /// Session-local definition identity.
        definition: SemanticItemId,
        /// Human label, never an identity key.
        label: String,
    },
    /// A primitive scalar.
    Scalar(String),
    /// The unit type.
    Unit,
    /// The never type.
    Never,
    /// Inference could not establish a type.
    Unknown,
    /// A known type outside the supported type projection.
    Unsupported,
}

/// Return contracts classified by semantic identity, including aliases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticReturnContract {
    /// The actual core `Result` definition with its instantiated arguments.
    Result {
        /// Identity of core's Result, not a name comparison.
        definition: SemanticItemId,
        /// Success type.
        success: SemanticType,
        /// Error type.
        error: SemanticType,
    },
    /// A return type that is not core Result.
    Other(SemanticType),
    /// Core Result identity or its arguments could not be established.
    Unavailable,
}

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

/// A call's resolved target, or the reason it cannot supply a source edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticCallTarget {
    /// A function or method definition, including imported aliases.
    Function(SemanticItemId),
    /// A callable such as a closure, constructor, or function pointer.
    OtherCallable,
    /// Resolution did not establish a callable.
    Unresolved,
}

/// One direct call expression, including its instantiated output contract.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct SemanticCall {
    range: super::SourceByteRange,
    target: SemanticCallTarget,
    output: SemanticReturnContract,
}

/// Function facts for one immutable IR snapshot, not an authorization to write.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    derive_getters::Getters,
    derive_new::new,
    derive_setters::Setters,
)]
#[new(visibility = "pub(crate)")]
#[setters(generate = false, prefix = "with_")]
pub struct SemanticFunction {
    definition: SemanticItemId,
    label: String,
    output: SemanticReturnContract,
    /// Formatting evidence from HIR, never inferred from printed type names.
    #[new(default)]
    #[serde(default)]
    #[setters(generate)]
    result_recording: SemanticResultRecording,
    /// Whether this declaration implements a core formatting trait.
    #[new(default)]
    #[serde(default)]
    #[setters(generate)]
    formatting_role: SemanticFormattingRole,
    /// Direct calls with their resolved targets and instantiated return types.
    #[new(default)]
    #[setters(generate)]
    calls: Vec<SemanticCall>,
    /// Result error types reachable in source calls, including handled errors.
    /// This is a reachability inventory, not proof that those errors escape.
    #[new(default)]
    #[setters(generate)]
    reachable_result_errors: Vec<SemanticType>,
    /// Resolution or a supported type projection was incomplete.
    #[new(default)]
    #[setters(generate)]
    incomplete: bool,
}
