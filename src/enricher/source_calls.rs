//! Resolve direct calls against the semantic facts retained in the shared IR.
//!
//! The syntactic calls of a function are joined, by range, to the targets
//! rust-analyzer resolved; no name is compared with another name. What this
//! stage still decides is the contract the transformations rely on: a direct,
//! unqualified call to a source free function in the caller's own module.
//!
//! The projection already resolves the calls of every function the ordinary build
//! has, one file at a time (see `loader::database::items`), so a session recomputes
//! them only for a file that changed. What is left for this stage is the functions
//! that exist only under a cfg context, whose facts arrive after the projection:
//! it resolves those, and does nothing when there are none.

use std::collections::HashMap;

use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{
    ATTR_FUNCTION_CALL_FLOW, ATTR_FUNCTION_SOURCE_SYNTAX, ATTR_SEMANTIC_FUNCTION,
    ATTR_SOURCE_MODULE_SCOPE, BasicQuery, EdgeKind, FunctionCallFlow, FunctionSourceSyntax, IrMut,
    IrView, ItemKind, NodeId, NodeKind, Resolved, ResolvedFunctionCall, SemanticFunction,
    SemanticItemId, SourceCallResolution, TargetFacts, resolve_call,
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
        // A function with syntax facts and no call flow: the projection did not
        // resolve it. That is a function only a cfg context activates.
        let mut pending: Vec<(NodeId, FunctionSourceSyntax)> = Vec::new();
        for node in &nodes {
            if node.attr(ATTR_FUNCTION_SOURCE_SYNTAX).is_some()
                && node.attr(ATTR_FUNCTION_CALL_FLOW).is_none()
                && let Some(facts) = ir.function_source_syntax(node.id())?
            {
                pending.push((node.id(), facts));
            }
        }
        if pending.is_empty() {
            return Ok(());
        }
        let mut definitions: HashMap<SemanticItemId, NodeId> = HashMap::new();
        let mut extern_crates = false;
        for node in nodes {
            // In older editions a root declaration can shadow an absolute
            // standard-library path. Refuse these names in every edition.
            extern_crates |= matches!(node.weight().name().as_deref(), Some("std" | "core"));
            if node.attr(ATTR_SOURCE_MODULE_SCOPE).is_some()
                && let Some(scope) = ir.source_module_scope(node.id())?
            {
                // Root macros/attributes can also introduce extern aliases
                // affecting absolute paths in files below the crate root.
                extern_crates |= scope.extern_crates()
                    || (node.weight().name().as_deref() == Some("") && scope.opaque());
            }
            if let Some(definition) = definition_of(ir, node.id())? {
                definitions.insert(definition, node.id());
            }
        }
        for (function, facts) in pending {
            enrich_function(ir, function, &facts, &definitions, extern_crates)?;
        }
        Ok(())
    }
}

/// The identity of the function a node is, from its ordinary facts or, when it
/// exists only in a cfg context, the first context's. The ordinary facts are
/// read for one field only: decoding every function's whole record to learn its
/// identity is what made this stage cost the size of the project.
#[tracing::instrument(level = "trace", skip(ir, function), err(level = "warn"))]
fn definition_of(ir: &dyn IrView, function: NodeId) -> CordialResult<Option<SemanticItemId>> {
    if let Some(facts) = ir
        .node(function)
        .and_then(|node| node.attr(ATTR_SEMANTIC_FUNCTION))
    {
        return Ok(facts["definition"].as_u64().map(SemanticItemId::new));
    }
    Ok(ir
        .semantic_context_functions(function)?
        .first()
        .map(|entry| *entry.function().definition()))
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
        let resolution = {
            let view: &dyn IrView = ir;
            resolve_call(
                namespace_trusted,
                facts,
                call,
                semantic.as_ref(),
                &mut |definition| {
                    let Some(target) = definitions.get(&definition).copied() else {
                        return Ok(None);
                    };
                    Ok(Some(TargetFacts {
                        key: target,
                        beside_the_caller: module.is_some_and(|module| {
                            view.parents(target, EdgeKind::Contains) == [module]
                        }),
                        is_function: view
                            .node(target)
                            .is_some_and(|node| node.kind() == &NodeKind::Item(ItemKind::Fn)),
                        syntax: view
                            .function_source_syntax(target)?
                            .map(std::borrow::Cow::Owned),
                    }))
                },
            )?
        };
        calls.push(ResolvedFunctionCall::new(
            call.range().clone(),
            match resolution {
                Resolved::Source {
                    target,
                    source_digest,
                } => SourceCallResolution::Source {
                    function: target,
                    source_digest,
                },
                Resolved::Unresolved(failure) => SourceCallResolution::Unresolved(failure),
            },
        ));
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
