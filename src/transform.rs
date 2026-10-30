//! Strategy operations planned from shared IR before composing a closed apply unit.
//!
//! These primitives deliberately have no disk writer or CLI entry point. A
//! return signature change creates caller obligations; its preview is not a
//! complete migration until a component planner has discharged them.

mod error_declaration;
mod failure_propagation;
mod generated_contract;
mod preview;
mod return_lift;
mod success_edits;
pub use error_declaration::{
    ErrorDeclarationBlocker, ErrorDeclarationDecision, GeneratedErrorNames, OwnedErrorDeclaration,
    plan_owned_error_declaration,
};
pub use failure_propagation::{
    FailurePropagationBlocker, FailurePropagationDecision, plan_result_unwrap_propagation,
};
pub use preview::{FunctionEdit, FunctionEditPlan};

pub use return_lift::{
    ReturnLiftBlocker, ReturnLiftDecision, ReturnLiftEdit, ReturnLiftPlan,
    plan_infallible_return_lift, plan_owned_error_return_lift,
};
