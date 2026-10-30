//! Render one represented error family using the existing nested source shape.

use crate::error::CordialResult;
use crate::ir::{ErrorFamilyMember, IrView, NodeId};

use super::{
    ErrorContractBroadening, ErrorDeclarationBlocker, ErrorFamilyLayoutDecision,
    ErrorFamilyLayoutPlan, ErrorFamilyRepresentationPlan, ErrorFamilyReusePolicy, FunctionEdit,
    FunctionEditPlan, plan_error_family_layout,
};

mod evidence;
mod names;
mod render;

/// Isolated declarations, an infallible root, or an undischarged obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorFamilyDeclarationDecision {
    /// Declarations and conversions; existing signatures and calls are untouched.
    Ready(OwnedErrorFamilyDeclaration),
    /// The root's represented error family is empty.
    Infallible,
    /// No partial edit plan is exposed on refusal.
    Refused(ErrorFamilyDeclarationBlocker),
}

/// Evidence or placement that prevents rendering the selected root family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorFamilyDeclarationBlocker {
    /// Exact-family representation still has unresolved obligations.
    Representation(ErrorFamilyRepresentationPlan),
    /// Source names, module facts, or concrete Error support are insufficient.
    Declaration(ErrorDeclarationBlocker),
    /// A nominal Result error is not a supported concrete Error implementation.
    UnsupportedSourceType(String),
}

/// A native alternative, retaining the complete shared identity and provenance.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct GeneratedErrorFamilySource {
    member: ErrorFamilyMember,
    variant: String,
    wrapper: String,
}

/// A child family carried as a native source wrapper, never a bare Kind.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct GeneratedErrorFamilyChild {
    #[getter(copy)]
    family: usize,
    variant: String,
    wrapper: String,
}

/// Names and alternatives for one family reachable from the selected root.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct GeneratedErrorFamily {
    /// Index into the accompanying layout, not this filtered declaration list.
    #[getter(copy)]
    family: usize,
    wrapper: String,
    kind: String,
    sources: Vec<GeneratedErrorFamilySource>,
    children: Vec<GeneratedErrorFamilyChild>,
}

/// One root and its child declarations, not a closed signature migration.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub struct OwnedErrorFamilyDeclaration {
    layout: ErrorFamilyLayoutPlan,
    #[getter(copy)]
    root: usize,
    families: Vec<GeneratedErrorFamily>,
    plan: FunctionEditPlan,
}

/// Render the selected root and its reachable child families from shared IR.
///
/// The root participates in analysis even when absent from `functions`. Other
/// represented families remain visible in the layout but are not emitted unless
/// reachable from this root. Consumers must share one unambiguous source module.
/// Supported native contracts are standard I/O, integer/float parsing and UTF-8
/// errors. Explicit From implementations route each native type along its unique
/// path, and child-to-parent conversions retain the whole child and its context.
/// Storage is explicitly owned; no caller closure, write authority, or borrowed
/// migration is implied by this declaration preview.
#[tracing::instrument(
    level = "debug",
    skip(ir, functions, root, reuse_policy, broadening_policy),
    err(level = "warn")
)]
#[hotpath::measure]
pub fn plan_owned_error_family_declaration(
    ir: &dyn IrView,
    functions: &[NodeId],
    root: NodeId,
    reuse_policy: ErrorFamilyReusePolicy,
    broadening_policy: ErrorContractBroadening,
) -> CordialResult<ErrorFamilyDeclarationDecision> {
    use ErrorFamilyDeclarationBlocker as Blocker;
    use ErrorFamilyDeclarationDecision::{Infallible, Ready, Refused};
    let mut functions = functions.to_vec();
    functions.push(root);
    functions.sort_by_key(|id| id.value());
    functions.dedup();
    let layout = match plan_error_family_layout(ir, &functions, reuse_policy, broadening_policy)? {
        ErrorFamilyLayoutDecision::Refused(representation) => {
            return Ok(Refused(Blocker::Representation(representation)));
        }
        ErrorFamilyLayoutDecision::Ready(layout) => layout,
    };
    let root_family = layout
        .representation()
        .reuse()
        .groups()
        .iter()
        .position(|group| group.functions().contains(&root))
        .and_then(|group| layout.contract_families()[group]);
    let Some(root_family) = root_family else {
        return Ok(Infallible);
    };
    let placement = match evidence::placement(ir, &functions, root)? {
        Ok(placement) => placement,
        Err(reason) => return Ok(Refused(Blocker::Declaration(reason))),
    };
    let families = match names::assign(&layout, root_family, placement.stem()) {
        Ok(families) => families,
        Err(reason) => return Ok(Refused(reason)),
    };
    if let Some(name) = names::collision(&families, placement.identifiers()) {
        return Ok(Refused(Blocker::Declaration(
            ErrorDeclarationBlocker::NameCollision(name.to_owned()),
        )));
    }
    let declaration = render::render(&layout, &families);
    Ok(Ready(OwnedErrorFamilyDeclaration {
        layout,
        root: root_family,
        families,
        plan: FunctionEditPlan::new(
            root,
            placement.path().clone(),
            placement.source_digest(),
            vec![FunctionEdit::new(
                (placement.offset()..placement.offset()).into(),
                declaration,
            )],
        ),
    }))
}
