//! Adapt a collected unit main to Rust's returned-error termination contract.

use crate::error::CordialResult;
use crate::ir::{CrateKind, EdgeKind, IrView, NodeId, SourceEntryContract};
use crate::objects::SourceSpan;

use super::{
    OwnedErrorDeclaration, ReturnLiftBlocker, ReturnLiftDecision, plan_owned_error_return_lift,
};

/// Lift a proven binary entry point to `Result<(), GeneratedError>`.
///
/// Success still terminates normally. Rust's Termination implementation reports
/// the returned error through Debug and supplies a failure exit status. This
/// operation handles the boundary in isolation; its caller obligations still
/// require component closure before writing.
#[tracing::instrument(level = "debug", skip(ir, main, declaration), err(level = "warn"))]
#[hotpath::measure]
pub fn plan_owned_main_return_lift(
    ir: &dyn IrView,
    main: NodeId,
    declaration: &OwnedErrorDeclaration,
) -> CordialResult<ReturnLiftDecision> {
    if !is_unit_main(ir, main)? {
        return Ok(ReturnLiftDecision::Refused(
            ReturnLiftBlocker::UnsupportedEntryPoint,
        ));
    }
    plan_owned_error_return_lift(ir, main, declaration)
}

#[tracing::instrument(level = "trace", skip(ir, main), err(level = "warn"))]
pub(super) fn is_unit_main(ir: &dyn IrView, main: NodeId) -> CordialResult<bool> {
    let Some(node) = ir.node(main) else {
        return Ok(false);
    };
    let (Some(facts), Some(entries), Some(span)) = (
        ir.function_source_syntax(main)?,
        ir.binary_entry_paths()?,
        node.weight().span(),
    ) else {
        return Ok(false);
    };
    if ir.crate_kinds()? != [CrateKind::Bin]
        || facts.entry_contract() != &Some(SourceEntryContract::UnitMain)
        || entries.as_slice() != [span.file().to_path_buf()]
        || node.weight().name().as_deref() != Some("main")
    {
        return Ok(false);
    }
    let parents = ir.parents(main, EdgeKind::Contains);
    if parents.len() != 1
        || ir.parents(parents[0], EdgeKind::Contains) != [ir.root()?]
        || !ir
            .node(parents[0])
            .is_some_and(|module| module.weight().name().as_deref() == Some(""))
    {
        return Ok(false);
    }
    Ok(ir
        .children(parents[0], EdgeKind::Contains)
        .iter()
        .filter(|id| {
            ir.node(**id)
                .is_some_and(|child| child.weight().name().as_deref() == Some("main"))
        })
        .count()
        == 1)
}
