//! Resolve direct calls against declarations already retained in the shared IR.

use std::collections::HashMap;

use crate::error::CordialResult;
use crate::hooks::{EnrichView, IrEnricher};
use crate::ir::{
    ATTR_FUNCTION_CALL_FLOW, BasicQuery, CallResolutionFailure, EdgeKind, FunctionBoundary,
    FunctionCallFlow, FunctionCallSyntax, FunctionSourceSyntax, IrMut, IrView, ItemKind, NodeId,
    NodeKind, ResolvedFunctionCall, SourceCallResolution,
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
        let mut names: HashMap<(NodeId, String), Vec<NodeId>> = HashMap::new();
        let mut functions = Vec::new();
        let mut extern_crates = false;
        for node in nodes {
            if let Some(scope) = ir.source_module_scope(node.id())? {
                // Root macros/attributes can also introduce extern aliases
                // affecting absolute paths in files below the crate root.
                extern_crates |= scope.extern_crates()
                    || (node.weight().name().as_deref() == Some("") && scope.opaque());
            }
            if let Some(facts) = ir.function_source_syntax(node.id())? {
                functions.push((node.id(), facts));
            }
            // Count all declarations, not only functions: a same-name const or
            // conditional duplicate must not be mistaken for one unique fn.
            if matches!(node.kind(), NodeKind::Item(_)) {
                for parent in ir.parents(node.id(), EdgeKind::Contains) {
                    if let Some(name) = node.weight().name() {
                        names
                            .entry((parent, name.clone()))
                            .or_default()
                            .push(node.id());
                    }
                }
            }
        }
        for (function, facts) in functions {
            enrich_function(ir, function, &facts, &names, extern_crates)?;
        }
        Ok(())
    }
}

#[tracing::instrument(level = "trace", skip(ir, function, facts, names), err(level = "warn"))]
fn enrich_function(
    ir: &mut dyn IrMut,
    function: NodeId,
    facts: &FunctionSourceSyntax,
    names: &HashMap<(NodeId, String), Vec<NodeId>>,
    extern_crates: bool,
) -> CordialResult<()> {
    let parents = ir.parents(function, EdgeKind::Contains);
    let module = (parents.len() == 1).then(|| parents[0]);
    let scope = module
        .map(|module| ir.source_module_scope(module))
        .transpose()?
        .flatten();
    let namespace_trusted = !extern_crates && scope.is_some_and(|scope| !scope.opaque());
    let mut calls = Vec::new();
    for call in facts.calls() {
        let resolution = resolve_call(ir, module, facts, call, names, namespace_trusted)?;
        if let SourceCallResolution::Source {
            function: target, ..
        } = &resolution
        {
            // Multiple call sites retain separate ranges, but graph adjacency
            // needs only one edge per caller/callee pair.
            if !ir.children(function, EdgeKind::Calls).contains(target) {
                ir.insert_edge(function, *target, EdgeKind::Calls)?;
            }
        }
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
    skip(ir, module, facts, call, names),
    err(level = "warn")
)]
fn resolve_call(
    ir: &dyn IrView,
    module: Option<NodeId>,
    facts: &FunctionSourceSyntax,
    call: &FunctionCallSyntax,
    names: &HashMap<(NodeId, String), Vec<NodeId>>,
    namespace_trusted: bool,
) -> CordialResult<SourceCallResolution> {
    use CallResolutionFailure as Failure;
    let unresolved = SourceCallResolution::Unresolved;
    if !namespace_trusted || *facts.has_call_shadowing() {
        return Ok(unresolved(Failure::OpaqueScope));
    }
    if let Some(boundary) = resolution_boundary(facts) {
        return Ok(unresolved(Failure::Boundary(boundary)));
    }
    let Some(name) = call.local_name() else {
        return Ok(unresolved(Failure::UnsupportedCallee));
    };
    let Some(candidates) = module.and_then(|module| names.get(&(module, name.clone()))) else {
        return Ok(unresolved(Failure::MissingOrAmbiguousTarget));
    };
    if candidates.len() != 1 {
        return Ok(unresolved(Failure::MissingOrAmbiguousTarget));
    }
    let target = candidates[0];
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
