//! Signature-level error alternatives and prospective panic-migration families.

use serde::{Deserialize, Serialize};

use super::{NodeId, SourceByteRange};

/// Enriched family on the same function node as its source and call facts.
pub const ATTR_FUNCTION_ERROR_FAMILY: &str = "function_error_family";

/// What the alternatives describe; these two contracts are not interchangeable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorFamilyBasis {
    /// Possible error type from an explicit standard Result signature. This
    /// does not assert that every error value is reachable in its body.
    DeclaredResult,
    /// Alternatives exposed by replacing inventoried unwraps with propagation
    /// and adapting ordinary callers. This is not today's return contract.
    PanicMigration,
}

/// One nominal type with all known producing declarations retained separately.
///
/// Provenance permits later domain-specific grouping; equal foreign types do
/// not establish that their producing operations have interchangeable meaning.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct ErrorFamilyMember {
    identity: String,
    origins: Vec<NodeId>,
}

/// Why known alternatives cannot be treated as an exhaustive family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ErrorFamilyGapReason {
    /// Source/call facts are absent or refer to different snapshots.
    MissingOrInvalidEvidence,
    /// A call has no uniquely resolved source declaration.
    UnresolvedCall,
    /// The declared Result error identity needs richer type resolution.
    UnresolvedErrorType,
    /// A method, macro, nested scope, or signature needs a dedicated contract.
    UnsupportedOperation,
    /// A Result is consumed outside the supported unwrap/handling operations.
    UnsupportedResultUse,
    /// A recognized Result operation has no resolved standard Result receiver.
    UnprovenResultReceiver,
}

/// A located obligation, retained through callers rather than silently dropped.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct ErrorFamilyGap {
    #[getter(copy)]
    function: NodeId,
    range: Option<SourceByteRange>,
    #[getter(copy)]
    reason: ErrorFamilyGapReason,
}

/// Shared analysis evidence, not a token authorizing a rewrite.
///
/// Members are sorted nominal alternatives with deduplicated origins. An empty
/// gap inventory means this bounded signature/migration analysis is complete;
/// control-flow reachability, Error implementations, conversions, storage, and
/// source-use closure remain separate transformation obligations.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
#[new(visibility = "pub(crate)")]
pub struct FunctionErrorFamily {
    #[getter(copy)]
    source_digest: [u8; 32],
    #[getter(copy)]
    basis: ErrorFamilyBasis,
    members: Vec<ErrorFamilyMember>,
    gaps: Vec<ErrorFamilyGap>,
}
