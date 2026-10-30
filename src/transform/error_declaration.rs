//! Construct an owned error chain from resolved source evidence.

use std::ops::ControlFlow;

use crate::error::CordialResult;
use crate::ir::{
    EdgeKind, FunctionSourceSyntax, FunctionUnwrapSyntax, IrView, ItemKind, NodeId, NodeKind,
};
use crate::objects::SourceSpan;

use super::failure_propagation::resolved_callee;
use super::{FailurePropagationBlocker, FunctionEdit, FunctionEditPlan};

mod names;
pub use names::GeneratedErrorNames;

/// Result of constructing declarations independently of signature migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorDeclarationDecision {
    /// New declarations and conversion, with no changes to existing functions.
    Ready(OwnedErrorDeclaration),
    /// No unwrap exists at the requested index.
    NoCandidate,
    /// No edits are published when an obligation is unresolved.
    Refused(ErrorDeclarationBlocker),
}

/// An obligation that this isolated declaration operation cannot discharge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorDeclarationBlocker {
    /// The same receiver proof required for failure propagation is unavailable.
    FailureEvidence(FailurePropagationBlocker),
    /// Only the known standard I/O error contract is currently supported.
    UnsupportedSourceType,
    /// Module insertion or naming evidence is absent or inconsistent.
    InvalidModuleEvidence,
    /// A declaration name cannot be derived by the current ASCII naming policy.
    UnsupportedName,
    /// A generated identifier is already written in the source module.
    NameCollision(String),
}

/// A source-preserving owned parent / Kind / native source declaration preview.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_new::new)]
#[new(visibility = "pub(super)")]
pub struct OwnedErrorDeclaration {
    names: GeneratedErrorNames,
    plan: FunctionEditPlan,
}

/// Generate a fresh owned error chain for one resolved standard I/O failure.
///
/// This operation selects owned storage explicitly; it does not implement the
/// borrowed policy or infer that any arbitrary Result error implements Error.
/// Declarations are private to the module. Signature and caller adaptations
/// remain separate obligations before a complete migration can be written.
#[tracing::instrument(level = "debug", skip(ir, function), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_owned_error_declaration(
    ir: &dyn IrView,
    function: NodeId,
    site_index: usize,
) -> CordialResult<ErrorDeclarationDecision> {
    use ErrorDeclarationBlocker as Blocker;
    use ErrorDeclarationDecision::{NoCandidate, Ready, Refused};
    let failure = |blocker| Refused(Blocker::FailureEvidence(blocker));
    let Some(node) = ir
        .node(function)
        .filter(|node| node.kind() == &NodeKind::Item(ItemKind::Fn))
    else {
        return Ok(failure(FailurePropagationBlocker::MissingFunctionEvidence));
    };
    let (Some(span), Some(facts)) = (node.weight().span(), ir.function_source_syntax(function)?)
    else {
        return Ok(failure(FailurePropagationBlocker::MissingFunctionEvidence));
    };
    let Some(site) = facts.unwraps().get(site_index) else {
        return Ok(NoCandidate);
    };
    if let Some(reason) = validate_source(ir, function, &facts, site)? {
        return Ok(Refused(reason));
    }
    let Some(names) = node
        .weight()
        .name()
        .as_deref()
        .and_then(|name| GeneratedErrorNames::for_site(name, site_index))
    else {
        return Ok(Refused(Blocker::UnsupportedName));
    };
    let parents = ir.parents(function, EdgeKind::Contains);
    let [module] = parents.as_slice() else {
        return Ok(Refused(Blocker::InvalidModuleEvidence));
    };
    let Some(scope) = ir.source_module_scope(*module)? else {
        return Ok(Refused(Blocker::InvalidModuleEvidence));
    };
    if scope.source_digest() != facts.source_digest()
        || scope.opaque()
        || scope.insertion_offset() < facts.body_range().end()
        || !ir.node(*module).is_some_and(|module| {
            module.kind() == &NodeKind::Module
                && module
                    .weight()
                    .span()
                    .as_ref()
                    .is_some_and(|module_span| module_span.file() == span.file())
        })
    {
        return Ok(Refused(Blocker::InvalidModuleEvidence));
    }
    if let Some(name) = names.collision(scope.identifiers()) {
        return Ok(Refused(Blocker::NameCollision(name.to_owned())));
    }
    let declaration = names.render();
    let offset = scope.insertion_offset();
    Ok(Ready(OwnedErrorDeclaration::new(
        names,
        FunctionEditPlan::new(
            function,
            span.file().to_path_buf(),
            facts.source_digest(),
            vec![FunctionEdit::new((offset..offset).into(), declaration)],
        ),
    )))
}

/// Require each grouped failure to share the generated native I/O contract.
#[tracing::instrument(level = "trace", skip(ir, function, facts, site), err(level = "warn"))]
pub(super) fn validate_source(
    ir: &dyn IrView,
    function: NodeId,
    facts: &FunctionSourceSyntax,
    site: &FunctionUnwrapSyntax,
) -> CordialResult<Option<ErrorDeclarationBlocker>> {
    let callee = match resolved_callee(ir, function, facts, site)? {
        ControlFlow::Break(reason) => {
            return Ok(Some(ErrorDeclarationBlocker::FailureEvidence(reason)));
        }
        ControlFlow::Continue(callee) => callee,
    };
    Ok((!callee
        .result_contract()
        .as_ref()
        .is_some_and(|contract| contract.error_identity().as_deref() == Some("::std::io::Error")))
    .then_some(ErrorDeclarationBlocker::UnsupportedSourceType))
}
