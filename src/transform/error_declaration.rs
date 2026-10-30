//! Construct an owned error chain from resolved source evidence.

use crate::error::CordialResult;
use crate::ir::{EdgeKind, IrView, ItemKind, NodeId, NodeKind};
use crate::objects::SourceSpan;

use super::{FailurePropagationBlocker, FunctionEdit, FunctionEditPlan, PanicSiteSelection};

mod names;
mod native;
pub use names::{GeneratedErrorNames, GeneratedErrorSource};

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
    /// The native type is not one of the supported concrete Error contracts.
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

/// Generate a fresh owned error chain for one supported native failure.
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
    plan_owned_error_group_declaration(ir, &[PanicSiteSelection::new(function, site_index)])
}

/// Construct one owned chain with a distinct wrapper per native source type.
///
/// Selections and native identities come from shared located call evidence.
/// Standard I/O and integer parsing errors are supported, including the core
/// ParseIntError reexport. Native alternatives are deduplicated and sorted.
/// No signature edits or failure-site rewrites are part of this operation.
#[tracing::instrument(level = "debug", skip(ir, sites), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_owned_error_group_declaration(
    ir: &dyn IrView,
    sites: &[PanicSiteSelection],
) -> CordialResult<ErrorDeclarationDecision> {
    use ErrorDeclarationBlocker as Blocker;
    use ErrorDeclarationDecision::{NoCandidate, Ready, Refused};
    let failure = |blocker| Refused(Blocker::FailureEvidence(blocker));
    let mut sites = sites.to_vec();
    sites.sort_by_key(|site| (site.function().value(), site.index()));
    let Some(first) = sites.first() else {
        return Ok(NoCandidate);
    };
    if sites.windows(2).any(|pair| pair[0] == pair[1]) {
        return Ok(Refused(Blocker::InvalidModuleEvidence));
    }
    let function = first.function();
    let site_index = first.index();
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
    let Some(_) = facts.unwraps().get(site_index) else {
        return Ok(NoCandidate);
    };
    let parents = ir.parents(function, EdgeKind::Contains);
    let [module] = parents.as_slice() else {
        return Ok(Refused(Blocker::InvalidModuleEvidence));
    };
    let mut natives = std::collections::BTreeSet::new();
    for selection in &sites {
        let Some(selected) = ir.function_source_syntax(selection.function())? else {
            return Ok(failure(FailurePropagationBlocker::MissingFunctionEvidence));
        };
        if ir.parents(selection.function(), EdgeKind::Contains) != parents
            || selected.source_digest() != facts.source_digest()
        {
            return Ok(Refused(Blocker::InvalidModuleEvidence));
        }
        let Some(site) = selected.unwraps().get(selection.index()) else {
            return Ok(failure(FailurePropagationBlocker::InvalidEvidence));
        };
        match native::resolve(ir, selection.function(), &selected, site)? {
            Ok(native) => {
                natives.insert(native);
            }
            Err(reason) => return Ok(Refused(reason)),
        }
    }
    let natives: Vec<_> = natives.into_iter().collect();
    let Some(names) = node
        .weight()
        .name()
        .as_deref()
        .and_then(|name| GeneratedErrorNames::for_site(name, site_index, &natives))
    else {
        return Ok(Refused(Blocker::UnsupportedName));
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
