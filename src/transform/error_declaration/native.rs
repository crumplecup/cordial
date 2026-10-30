//! Supported native source contracts, resolved from shared call facts.

use std::ops::ControlFlow;

use super::ErrorDeclarationBlocker as Blocker;
use crate::error::CordialResult;
use crate::ir::{ErrorFamilyBasis, FunctionSourceSyntax, FunctionUnwrapSyntax, IrView, NodeId};

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
    let (target, callee) =
        match super::super::call_target::resolved_call(ir, function, facts, site.receiver())? {
            ControlFlow::Break(reason) => return Ok(Err(Blocker::FailureEvidence(reason))),
            ControlFlow::Continue(callee) => callee,
        };
    if callee.result_contract().is_none() {
        return Ok(Err(Blocker::UnsupportedSourceType));
    }
    let Some(family) = ir.function_error_family(target)? else {
        return Ok(Err(Blocker::FailureEvidence(
            super::super::FailurePropagationBlocker::MissingCallEvidence,
        )));
    };
    if family.source_digest() != callee.source_digest()
        || family.basis() != ErrorFamilyBasis::DeclaredResult
    {
        return Ok(Err(Blocker::FailureEvidence(
            super::super::FailurePropagationBlocker::InvalidEvidence,
        )));
    }
    if !family.gaps().is_empty() || family.members().len() != 1 {
        return Ok(Err(Blocker::UnsupportedSourceType));
    }
    let member = &family.members()[0];
    if member.origins() != &[target]
        || callee
            .result_contract()
            .as_ref()
            .and_then(|contract| contract.canonical_error_identity())
            != Some(member.identity().as_str())
    {
        return Ok(Err(Blocker::FailureEvidence(
            super::super::FailurePropagationBlocker::InvalidEvidence,
        )));
    }
    Ok(match member.identity().as_str() {
        "::std::io::Error" => Ok(NativeError::Io),
        "::std::num::ParseIntError" => Ok(NativeError::ParseInt),
        _ => Err(Blocker::UnsupportedSourceType),
    })
}
