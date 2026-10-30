//! Strategy operations planned from shared IR before composing a closed apply unit.
//!
//! These primitives deliberately have no disk writer or CLI entry point. A
//! return signature change creates caller obligations; its preview is not a
//! complete migration until a component planner has discharged them.

mod call_target;
mod caller_adaptation;
mod closed_panic;
mod error_declaration;
mod failure_propagation;
mod generated_contract;
mod main_boundary;
mod panic_component;
mod preview;
mod return_lift;
mod success_edits;
pub use caller_adaptation::{
    CallerAdaptationBlocker, CallerAdaptationDecision, OwnedCallerAdaptation,
    plan_owned_caller_adaptation,
};
pub use closed_panic::{
    ClosedPanicBlocker, ClosedPanicDecision, ClosedPanicMigration,
    plan_closed_owned_panic_component,
};
pub use error_declaration::{
    ErrorDeclarationBlocker, ErrorDeclarationDecision, GeneratedErrorNames, OwnedErrorDeclaration,
    plan_owned_error_declaration,
};
pub use failure_propagation::{
    FailurePropagationBlocker, FailurePropagationDecision, plan_result_unwrap_propagation,
};
pub use main_boundary::plan_owned_main_return_lift;
pub use panic_component::{
    PanicComponentBlocker, PanicComponentDecision, PanicComponentPreview,
    plan_owned_panic_component,
};
pub use preview::{FunctionEdit, FunctionEditPlan};

pub use return_lift::{
    ReturnLiftBlocker, ReturnLiftDecision, ReturnLiftEdit, ReturnLiftPlan,
    plan_infallible_return_lift, plan_owned_error_return_lift,
};
