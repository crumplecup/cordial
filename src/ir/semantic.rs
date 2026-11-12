//! Snapshot-local semantic evidence projected from the shared source database.

use serde::{Deserialize, Serialize};

use super::semantic_callee::SemanticCallee;
use super::semantic_macros::SemanticMacroCall;
use super::semantic_recording::{
    SemanticFormattingRole, SemanticParameter, SemanticResultRecording, SemanticReturnRecording,
    SemanticReturnShape,
};

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
    workspace: super::SemanticWorkspaceContext,
}

/// Function evidence produced by the canonical semantic database.
pub const ATTR_SEMANTIC_FUNCTION: &str = "semantic_function";

/// An interned definition identity, meaningful only inside its analysis session.
/// Labels and printed type paths are never used to decide identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, derive_new::new)]
#[new(visibility = "pub(crate)")]
pub struct SemanticItemId(u64);

impl SemanticItemId {
    /// The raw bits of the session-local identity.
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn bits(self) -> u64 {
        self.0
    }
}

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
pub struct SemanticCall {
    range: super::SourceByteRange,
    target: SemanticCallTarget,
    output: SemanticReturnContract,
    /// What the call resolved to, when it resolved to a function.
    #[new(default)]
    #[serde(default)]
    #[setters(generate)]
    callee: Option<SemanticCallee>,
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
    /// Every macro invocation in the body and the macro it resolved to.
    #[new(default)]
    #[serde(default)]
    #[setters(generate)]
    macro_calls: Vec<SemanticMacroCall>,
    /// What the return type is, for role classification.
    #[new(default)]
    #[serde(default)]
    #[setters(generate)]
    return_shape: SemanticReturnShape,
    /// Whether the return value can be recorded, and whether it is borrowed.
    #[new(default)]
    #[serde(default)]
    #[setters(generate)]
    return_recording: SemanticReturnRecording,
    /// The bindings of the parameters, in order, with their `Debug` verdicts.
    #[new(default)]
    #[serde(default)]
    #[setters(generate)]
    parameters: Vec<SemanticParameter>,
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
