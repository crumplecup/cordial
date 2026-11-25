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
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    derive_new::new,
)]
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

/// A part of a function's facts that two versions of them can differ in. Calls say how.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FactPart {
    /// The function's identity.
    Definition,
    /// The function's label.
    Label,
    /// The return contract.
    Output,
    /// Formatting evidence for the result.
    ResultRecording,
    /// Which formatting trait it implements.
    FormattingRole,
    /// The macros called in the body and what they resolved to.
    MacroCalls,
    /// What the return type is.
    ReturnShape,
    /// Whether the return value can be recorded.
    ReturnRecording,
    /// The parameters and their `Debug` verdicts.
    Parameters,
    /// There are more or fewer calls.
    CallsCount,
    /// A call resolved to another target.
    CallsTarget,
    /// A call is somewhere else in the file.
    CallsRange,
    /// A call has another return contract.
    CallsOutput,
    /// A call has another callee.
    CallsCallee,
    /// The result error types reachable in calls.
    ReachableResultErrors,
    /// Whether resolution was incomplete.
    Incomplete,
}

impl SemanticFunction {
    /// The parts of these facts that are not the same in `other`, in a fixed order; none when they are equal.
    #[tracing::instrument(level = "trace", skip(self, other))]
    pub(crate) fn parts_that_differ(&self, other: &Self) -> Vec<FactPart> {
        let pairs = || self.calls.iter().zip(&other.calls);
        let same_count = self.calls.len() == other.calls.len();
        [
            (self.definition != other.definition, FactPart::Definition),
            (self.label != other.label, FactPart::Label),
            (self.output != other.output, FactPart::Output),
            (self.result_recording != other.result_recording, FactPart::ResultRecording),
            (self.formatting_role != other.formatting_role, FactPart::FormattingRole),
            (self.macro_calls != other.macro_calls, FactPart::MacroCalls),
            (self.return_shape != other.return_shape, FactPart::ReturnShape),
            (self.return_recording != other.return_recording, FactPart::ReturnRecording),
            (self.parameters != other.parameters, FactPart::Parameters),
            (!same_count, FactPart::CallsCount),
            (same_count && pairs().any(|(a, b)| a.target != b.target), FactPart::CallsTarget),
            (same_count && pairs().any(|(a, b)| a.range != b.range), FactPart::CallsRange),
            (same_count && pairs().any(|(a, b)| a.output != b.output), FactPart::CallsOutput),
            (same_count && pairs().any(|(a, b)| a.callee != b.callee), FactPart::CallsCallee),
            (self.reachable_result_errors != other.reachable_result_errors, FactPart::ReachableResultErrors),
            (self.incomplete != other.incomplete, FactPart::Incomplete),
        ]
        .into_iter()
        .filter_map(|(differs, part)| differs.then_some(part))
        .collect()
    }
}

impl SemanticFunction {
    /// What `part` is in these facts and in `other`'s, for the log: the first call that differs for a call part, and
    /// the whole part otherwise, each cut short.
    #[tracing::instrument(level = "trace", skip(self, other))]
    pub(crate) fn difference_in(&self, other: &Self, part: FactPart) -> String {
        fn short(value: &dyn std::fmt::Debug) -> String {
            format!("{value:?}").chars().take(240).collect()
        }
        let first_call = || {
            self.calls
                .iter()
                .zip(&other.calls)
                .find(|(a, b)| a != b)
                .map(|(a, b)| format!("{} -> {}", short(a), short(b)))
                .unwrap_or_default()
        };
        match part {
            FactPart::Definition => format!("{} -> {}", short(&self.definition), short(&other.definition)),
            FactPart::Label => format!("{} -> {}", short(&self.label), short(&other.label)),
            FactPart::Output => format!("{} -> {}", short(&self.output), short(&other.output)),
            FactPart::ResultRecording => {
                format!("{} -> {}", short(&self.result_recording), short(&other.result_recording))
            }
            FactPart::FormattingRole => {
                format!("{} -> {}", short(&self.formatting_role), short(&other.formatting_role))
            }
            FactPart::MacroCalls => format!("{} -> {}", short(&self.macro_calls), short(&other.macro_calls)),
            FactPart::ReturnShape => format!("{} -> {}", short(&self.return_shape), short(&other.return_shape)),
            FactPart::ReturnRecording => {
                format!("{} -> {}", short(&self.return_recording), short(&other.return_recording))
            }
            FactPart::Parameters => format!("{} -> {}", short(&self.parameters), short(&other.parameters)),
            FactPart::CallsCount => format!("{} calls -> {} calls", self.calls.len(), other.calls.len()),
            FactPart::CallsTarget | FactPart::CallsRange | FactPart::CallsOutput | FactPart::CallsCallee => {
                first_call()
            }
            FactPart::ReachableResultErrors => format!(
                "{} -> {}",
                short(&self.reachable_result_errors),
                short(&other.reachable_result_errors)
            ),
            FactPart::Incomplete => format!("{} -> {}", self.incomplete, other.incomplete),
        }
    }
}

impl SemanticCall {
    /// This call with its range moved forward by `anchor`; a call whose range is no coordinate
    /// of any file (see `EXPANSION_RANGE`) is returned as it is.
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn moved_by(&self, anchor: usize) -> Self {
        if self.range.start() == usize::MAX {
            return self.clone();
        }
        Self {
            range: (self.range.start() + anchor..self.range.end() + anchor).into(),
            ..self.clone()
        }
    }
}

impl SemanticFunction {
    /// This evidence with the range of every call moved forward by `anchor`: the evidence is
    /// kept with each range relative to where its function starts, so that text written above
    /// the function does not change it, and is given its place in the file when it is used.
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn with_ranges_from(&self, anchor: usize) -> Self {
        let calls = self
            .calls
            .iter()
            .map(|call| call.moved_by(anchor))
            .collect();
        self.clone().with_calls(calls)
    }
}

impl SemanticType {
    /// This type with each identity in it replaced by `map`'s answer.
    #[tracing::instrument(level = "trace", skip(self, map))]
    pub(crate) fn map_identities(
        &self,
        map: &mut impl FnMut(SemanticItemId) -> SemanticItemId,
    ) -> Self {
        match self {
            Self::Adt { definition, label } => Self::Adt {
                definition: map(*definition),
                label: label.clone(),
            },
            other => other.clone(),
        }
    }
}

impl SemanticReturnContract {
    /// This contract with each identity in it replaced by `map`'s answer.
    #[tracing::instrument(level = "trace", skip(self, map))]
    pub(crate) fn map_identities(
        &self,
        map: &mut impl FnMut(SemanticItemId) -> SemanticItemId,
    ) -> Self {
        match self {
            Self::Result {
                definition,
                success,
                error,
            } => Self::Result {
                definition: map(*definition),
                success: success.map_identities(map),
                error: error.map_identities(map),
            },
            Self::Other(ty) => Self::Other(ty.map_identities(map)),
            Self::Unavailable => Self::Unavailable,
        }
    }
}

impl SemanticFunction {
    /// These facts with every identity in them (the function's own, the types it
    /// returns and reaches, the functions it calls) replaced by `map`'s answer.
    /// A context analyses a copy of the crate, so its facts name the copy's items;
    /// mapping them to the ordinary build's makes them the same facts about the
    /// same functions and types.
    #[tracing::instrument(level = "debug", skip(self, map))]
    #[hotpath::measure]
    pub(crate) fn map_identities(
        mut self,
        map: &mut impl FnMut(SemanticItemId) -> SemanticItemId,
    ) -> Self {
        self.definition = map(self.definition);
        self.output = self.output.map_identities(map);
        for call in &mut self.calls {
            if let SemanticCallTarget::Function(target) = call.target {
                call.target = SemanticCallTarget::Function(map(target));
            }
            call.output = call.output.map_identities(map);
        }
        self.reachable_result_errors = self
            .reachable_result_errors
            .iter()
            .map(|ty| ty.map_identities(map))
            .collect();
        self
    }
}
