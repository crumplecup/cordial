//! Located calls retained from the shared tree, without guessing receiver types.

use serde::{Deserialize, Serialize};

use super::SourceByteRange;

mod compatibility;
mod native;
pub(super) use compatibility::collect;
pub(super) use native::collect as collect_native;

/// A call expression belonging to this function's execution scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionCallSyntax {
    range: SourceByteRange,
    /// Only an unqualified, nongeneric path is eligible for the first resolver.
    local_name: Option<String>,
    /// Missing in older snapshots; absence is not evidence of ordinary use.
    #[serde(default)]
    usage: Option<SourceCallUse>,
}

/// How a directly enclosing expression consumes a call's returned value.
///
/// These are syntax facts. Enrichment must establish a standard Result receiver
/// before treating a method as handling an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceCallUse {
    /// Ordinary value use, including arguments of another call.
    Value,
    /// Direct receiver of zero-argument `unwrap`.
    Unwrap,
    /// Direct operand of `?`; conversion still needs proof.
    Residual,
    /// Direct receiver of `unwrap_or`, `unwrap_or_default`, `is_ok`, or `is_err`.
    HandledResult,
}

/// A zero-argument, nongeneric `unwrap` candidate; this does not prove Result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct FunctionUnwrapSyntax {
    /// Source line from the shared tree; absent in older cache versions.
    #[serde(default)]
    #[getter(copy)]
    line: Option<u32>,
    range: SourceByteRange,
    receiver: SourceByteRange,
    /// Dot, method name, and delimiters; comments between them are retained.
    suffix_tokens: Vec<SourceByteRange>,
}
