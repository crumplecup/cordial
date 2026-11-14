//! Which function a written call names, decided from facts alone.
//!
//! The decision is a function of the call's syntax, the calling function's
//! semantic facts, and a few things about the target. It does not matter where
//! those come from: the source-call enricher reads them out of the graph, and the
//! projection reads them out of one file's items. Both call this, so the rules
//! exist once.

use super::CallResolutionFailure;
use crate::CordialResult;
use crate::ir::{
    FunctionBoundary, FunctionCallSyntax, FunctionSourceSyntax, SemanticCallTarget,
    SemanticFunction, SemanticItemId,
};

/// What the resolver needs to know about the function a call resolved to.
#[derive(Debug, Clone)]
pub(crate) struct TargetFacts<K> {
    /// How the caller refers to it: a graph node, or a place in a file's items.
    pub(crate) key: K,
    /// Whether it is declared in the calling function's own module.
    pub(crate) beside_the_caller: bool,
    /// Whether it is a function item.
    pub(crate) is_function: bool,
    /// Its syntax facts, when it has them (top-level free functions do).
    pub(crate) syntax: Option<FunctionSourceSyntax>,
}

#[derive(serde::Serialize, serde::Deserialize)]
/// The resolution of one call, naming a target by `K`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Resolved<K> {
    /// A direct call to a source free function in the same module.
    Source {
        /// The target.
        target: K,
        /// The target's snapshot digest.
        source_digest: [u8; 32],
    },
    /// The call stays visible without authorizing a transform.
    Unresolved(CallResolutionFailure),
}

/// Resolve `call`, written in a function with syntax `facts` and semantic facts
/// `semantic`. `target_of` says what is known about a definition the call
/// resolved to, or `None` when it names nothing here.
#[tracing::instrument(level = "trace", skip_all, err(level = "warn"))]
pub(crate) fn resolve_call<K: Copy>(
    namespace_trusted: bool,
    facts: &FunctionSourceSyntax,
    call: &FunctionCallSyntax,
    semantic: Option<&SemanticFunction>,
    target_of: &mut dyn FnMut(SemanticItemId) -> CordialResult<Option<TargetFacts<K>>>,
) -> CordialResult<Resolved<K>> {
    use CallResolutionFailure as Failure;
    let unresolved = Resolved::Unresolved;
    // An untrusted namespace withholds write authority, not resolution: the
    // `Calls` edge is the database's, and stays.
    if !namespace_trusted || *facts.has_call_shadowing() {
        return Ok(unresolved(Failure::OpaqueScope));
    }
    if let Some(boundary) = resolution_boundary(facts) {
        return Ok(unresolved(Failure::Boundary(boundary)));
    }
    if call.local_name().is_none() {
        return Ok(unresolved(Failure::UnsupportedCallee));
    }
    // Without semantic facts the compiler front end has not established what the
    // names in this function mean.
    let Some(semantic) = semantic else {
        tracing::debug!(
            target: "cordial::semantic::calls",
            call = ?call.range(),
            "no semantic facts for the calling function; call left unresolved"
        );
        return Ok(unresolved(Failure::OpaqueScope));
    };
    let Some(resolved) = semantic
        .calls()
        .iter()
        .find(|resolved| resolved.range() == call.range())
    else {
        return Ok(unresolved(Failure::MissingOrAmbiguousTarget));
    };
    let SemanticCallTarget::Function(definition) = resolved.target() else {
        return Ok(unresolved(match resolved.target() {
            SemanticCallTarget::OtherCallable => Failure::UnsupportedCallee,
            _ => Failure::MissingOrAmbiguousTarget,
        }));
    };
    let Some(target) = target_of(*definition)? else {
        return Ok(unresolved(Failure::MissingOrAmbiguousTarget));
    };
    // The transformations adapt callers declared beside the callee.
    if !target.beside_the_caller {
        return Ok(unresolved(Failure::MissingOrAmbiguousTarget));
    }
    if !target.is_function {
        return Ok(unresolved(Failure::UnsupportedCallee));
    }
    let Some(target_facts) = target.syntax else {
        return Ok(unresolved(Failure::UnsupportedCallee));
    };
    if let Some(boundary) = resolution_boundary(&target_facts) {
        return Ok(unresolved(Failure::Boundary(boundary)));
    }
    Ok(Resolved::Source {
        target: target.key,
        source_digest: target_facts.source_digest(),
    })
}

/// The first boundary that makes a function's contract one this resolver does
/// not interpret.
#[tracing::instrument(level = "trace", skip(facts))]
pub(crate) fn resolution_boundary(facts: &FunctionSourceSyntax) -> Option<FunctionBoundary> {
    facts
        .return_flow()
        .boundaries()
        .iter()
        .copied()
        .find(|boundary| {
            matches!(
                boundary,
                FunctionBoundary::SpecialSignature
                    | FunctionBoundary::Generic
                    | FunctionBoundary::Attribute
                    | FunctionBoundary::Macro
            )
        })
}
