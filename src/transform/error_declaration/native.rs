//! Supported native source contracts, resolved from shared call facts.

use std::ops::ControlFlow;

use super::ErrorDeclarationBlocker as Blocker;
use crate::error::CordialResult;
use crate::ir::{FunctionSourceSyntax, FunctionUnwrapSyntax, IrView, NodeId};

/// A known Error implementation; arbitrary absolute types do not prove Error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum NativeError {
    Io,
    ParseInt,
}

impl NativeError {
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn identity(self) -> &'static str {
        match self {
            Self::Io => "::std::io::Error",
            Self::ParseInt => "::std::num::ParseIntError",
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn variant(self) -> &'static str {
        match self {
            Self::Io => "Io",
            Self::ParseInt => "ParseInt",
        }
    }
}

#[tracing::instrument(level = "trace", skip(ir, function, facts, site), err(level = "warn"))]
pub(super) fn resolve(
    ir: &dyn IrView,
    function: NodeId,
    facts: &FunctionSourceSyntax,
    site: &FunctionUnwrapSyntax,
) -> CordialResult<Result<NativeError, Blocker>> {
    let callee =
        match super::super::failure_propagation::resolved_callee(ir, function, facts, site)? {
            ControlFlow::Break(reason) => return Ok(Err(Blocker::FailureEvidence(reason))),
            ControlFlow::Continue(callee) => callee,
        };
    Ok(
        match callee
            .result_contract()
            .as_ref()
            .and_then(|contract| contract.error_identity().as_deref())
        {
            Some("::std::io::Error") => Ok(NativeError::Io),
            Some("::std::num::ParseIntError" | "::core::num::ParseIntError") => {
                Ok(NativeError::ParseInt)
            }
            _ => Err(Blocker::UnsupportedSourceType),
        },
    )
}
