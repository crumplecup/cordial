//! Resolve direct calls against the semantic facts retained in the shared IR.
//!
//! The syntactic calls of a function are joined, by range, to the targets
//! rust-analyzer resolved; no name is compared with another name. What this
//! stage still decides is the contract the transformations rely on: a direct,
//! unqualified call to a source free function in the caller's own module.

use std::collections::HashMap;

use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{
    ATTR_FUNCTION_CALL_FLOW, BasicQuery, CallResolutionFailure, EdgeKind, FunctionBoundary,
    FunctionCallFlow, FunctionCallSyntax, FunctionSourceSyntax, IrMut, IrView, ItemKind, NodeId,
    NodeKind, ResolvedFunctionCall, SemanticCallTarget, SemanticFunction, SemanticItemId,
    SourceCallResolution,
};

/// Adds source declaration identities and caller edges without another parse.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceCallEnricher;

impl IrEnricher for SourceCallEnricher {
    #[tracing::instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "source-calls"
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn priority(&self) -> u8 {
        2
    }

    #[tracing::instrument(level = "trace", skip(self, view))]
    #[hotpath::measure(impl_type = "SourceCallEnricher")]
    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let ir = view.ir();
        let nodes = ir.nodes_matching(&BasicQuery::all_nodes());
        let mut definitions: HashMap<SemanticItemId, NodeId> = HashMap::new();
        let mut functions = Vec::new();
        let mut extern_crates = false;
        for node in nodes {
            // In older editions a root declaration can shadow an absolute
            // standard-library path. Refuse these names in every edition.
            extern_crates |= matches!(node.weight().name().as_deref(), Some("std" | "core"));
            if let Some(scope) = ir.source_module_scope(node.id())? {
                // Root macros/attributes can also introduce extern aliases
                // affecting absolute paths in files below the crate root.
                extern_crates |= scope.extern_crates()
                    || (node.weight().name().as_deref() == Some("") && scope.opaque());
            }
            if let Some(facts) = ir.function_source_syntax(node.id())? {
                functions.push((node.id(), facts));
            }
            if let Some(semantic) = semantic_facts(ir, node.id())? {
                definitions.insert(*semantic.definition(), node.id());
            }
        }
        for (function, facts) in functions {
            enrich_function(ir, function, &facts, &definitions, extern_crates)?;
        }
        Ok(())
    }
}

/// A function's semantic facts: the ordinary build's, or those of the first cfg
/// context that resolved it when it exists only there.
#[tracing::instrument(level = "trace", skip(ir, function), err(level = "warn"))]
fn semantic_facts(ir: &dyn IrView, function: NodeId) -> CordialResult<Option<SemanticFunction>> {
    if let Some(facts) = ir.semantic_function(function)? {
        return Ok(Some(facts));
    }
    Ok(ir
        .semantic_context_functions(function)?
        .first()
        .map(|entry| entry.function().clone()))
}

#[tracing::instrument(
    level = "trace",
    skip(ir, function, facts, definitions),
    err(level = "warn")
)]
fn enrich_function(
    ir: &mut dyn IrMut,
    function: NodeId,
    facts: &FunctionSourceSyntax,
    definitions: &HashMap<SemanticItemId, NodeId>,
    extern_crates: bool,
) -> CordialResult<()> {
    let parents = ir.parents(function, EdgeKind::Contains);
    let module = (parents.len() == 1).then(|| parents[0]);
    let scope = module
        .map(|module| ir.source_module_scope(module))
        .transpose()?
        .flatten();
    let namespace_trusted = !extern_crates && scope.is_some_and(|scope| !scope.opaque());
    let semantic = semantic_facts(ir, function)?;
    let mut calls = Vec::new();
    for call in facts.calls() {
        let resolution = resolve_call(
            ir,
            module,
            facts,
            call,
            semantic.as_ref(),
            definitions,
            namespace_trusted,
        )?;
        calls.push(ResolvedFunctionCall::new(call.range().clone(), resolution));
    }
    ir.set_attr(
        function,
        ATTR_FUNCTION_CALL_FLOW,
        serde_json::to_value(FunctionCallFlow::new(
            facts.source_digest(),
            namespace_trusted,
            calls,
        ))?,
    )?;
    Ok(())
}

#[tracing::instrument(
    level = "trace",
    skip(ir, module, facts, call, semantic, definitions, namespace_trusted),
    err(level = "warn")
)]
fn resolve_call(
    ir: &dyn IrView,
    module: Option<NodeId>,
    facts: &FunctionSourceSyntax,
    call: &FunctionCallSyntax,
    semantic: Option<&SemanticFunction>,
    definitions: &HashMap<SemanticItemId, NodeId>,
    namespace_trusted: bool,
) -> CordialResult<SourceCallResolution> {
    use CallResolutionFailure as Failure;
    let unresolved = SourceCallResolution::Unresolved;
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
    let Some(target) = definitions.get(definition).copied() else {
        return Ok(unresolved(Failure::MissingOrAmbiguousTarget));
    };
    // The transformations adapt callers declared beside the callee.
    if module.is_none_or(|module| ir.parents(target, EdgeKind::Contains) != [module]) {
        return Ok(unresolved(Failure::MissingOrAmbiguousTarget));
    }
    if !ir
        .node(target)
        .is_some_and(|node| node.kind() == &NodeKind::Item(ItemKind::Fn))
    {
        return Ok(unresolved(Failure::UnsupportedCallee));
    }
    let Some(target_facts) = ir.function_source_syntax(target)? else {
        return Ok(unresolved(Failure::UnsupportedCallee));
    };
    if let Some(boundary) = resolution_boundary(&target_facts) {
        return Ok(unresolved(Failure::Boundary(boundary)));
    }
    Ok(SourceCallResolution::Source {
        function: target,
        source_digest: target_facts.source_digest(),
    })
}

#[tracing::instrument(level = "trace", skip(facts))]
fn resolution_boundary(facts: &FunctionSourceSyntax) -> Option<FunctionBoundary> {
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
