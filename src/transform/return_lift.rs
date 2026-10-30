//! Lift successful returns without discovering syntax downstream of the IR.

use std::ops::ControlFlow;
use std::path::PathBuf;

use super::success_edits::edits_from_facts;

use sha2::{Digest, Sha256};

use crate::error::{CordialError, CordialResult};
use crate::ir::{FunctionBoundary, IrView, ItemKind, NodeId, NodeKind, SourceByteRange};
use crate::objects::SourceSpan;

/// A located replacement in the collected source snapshot.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
pub struct ReturnLiftEdit {
    range: SourceByteRange,
    replacement: String,
}

/// An isolated successful-return lift; caller obligations remain outstanding.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct ReturnLiftPlan {
    #[getter(copy)]
    function: NodeId,
    path: PathBuf,
    #[getter(copy)]
    source_digest: [u8; 32],
    edits: Vec<ReturnLiftEdit>,
}

/// Result of planning one isolated return operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnLiftDecision {
    /// Located edits, without permission to write an incomplete migration.
    Ready(ReturnLiftPlan),
    /// The exact generated wrapper is already present; no edits are needed.
    AlreadyLifted,
    /// No partial plan is returned when an obligation is unsupported.
    Refused(ReturnLiftBlocker),
}

/// Evidence that prevents this operation from being planned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnLiftBlocker {
    /// A source function, span, or its typed facts are missing.
    MissingFunctionEvidence,
    /// The function has a contract requiring a different operation.
    Boundary(FunctionBoundary),
    /// Ranges are inconsistent with their owning signature, body, or returns.
    InvalidRanges,
}

/// Plan `T` to `Result<T, Infallible>` using only collected IR facts.
///
/// This supplies the success adapter in isolation. It neither proves caller
/// closure nor selects an error architecture for a real failure source.
#[tracing::instrument(level = "debug", skip(ir, function), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_infallible_return_lift(
    ir: &dyn IrView,
    function: NodeId,
) -> CordialResult<ReturnLiftDecision> {
    let Some(node) = ir.node(function) else {
        return Ok(ReturnLiftDecision::Refused(
            ReturnLiftBlocker::MissingFunctionEvidence,
        ));
    };
    let Some(span) = node.weight().span().as_ref() else {
        return Ok(ReturnLiftDecision::Refused(
            ReturnLiftBlocker::MissingFunctionEvidence,
        ));
    };
    if node.kind() != &NodeKind::Item(ItemKind::Fn) {
        return Ok(ReturnLiftDecision::Refused(
            ReturnLiftBlocker::MissingFunctionEvidence,
        ));
    }
    let Some(facts) = ir.function_source_syntax(function)? else {
        return Ok(ReturnLiftDecision::Refused(
            ReturnLiftBlocker::MissingFunctionEvidence,
        ));
    };
    if facts.has_infallible_result_syntax() {
        return Ok(ReturnLiftDecision::AlreadyLifted);
    }
    if let Some(boundary) = facts.return_flow().boundaries().first() {
        return Ok(ReturnLiftDecision::Refused(ReturnLiftBlocker::Boundary(
            *boundary,
        )));
    }
    let edits = match edits_from_facts(&facts) {
        ControlFlow::Continue(edits) => edits,
        ControlFlow::Break(blocker) => return Ok(ReturnLiftDecision::Refused(blocker)),
    };
    Ok(ReturnLiftDecision::Ready(ReturnLiftPlan {
        function,
        path: span.file().to_path_buf(),
        source_digest: facts.source_digest(),
        edits,
    }))
}

impl ReturnLiftPlan {
    /// Render a deterministic preview against the exact collected bytes.
    ///
    /// Reads text only for located edits; does not parse or rediscover evidence.
    /// No source files or worklist claims are changed by this method.
    #[tracing::instrument(level = "debug", skip(self, source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "ReturnLiftPlan")]
    pub fn preview(&self, source: &str) -> CordialResult<String> {
        let digest: [u8; 32] = Sha256::digest(source.as_bytes()).into();
        if digest != self.source_digest {
            return Err(CordialError::stale_source_snapshot(&self.path));
        }
        // Validate every range before constructing any output, including UTF-8
        // boundaries. Malformed persisted facts must never panic in replace_range.
        let mut previous_end = 0;
        for edit in &self.edits {
            let range = edit.range.start()..edit.range.end();
            if range.start < previous_end || source.get(range.clone()).is_none() {
                return Err(std::io::Error::from(std::io::ErrorKind::InvalidData).into());
            }
            previous_end = range.end;
        }
        let mut output = source.to_owned();
        for edit in self.edits.iter().rev() {
            output.replace_range(edit.range.start()..edit.range.end(), &edit.replacement);
        }
        Ok(output)
    }
}
