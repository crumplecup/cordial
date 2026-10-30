#![cfg(feature = "panics")]

use cordial::{
    ATTR_FUNCTION_SOURCE_SYNTAX, ErrorDeclarationDecision, FunctionBoundary, IrView,
    ReturnLiftBlocker as Blocker, ReturnLiftDecision as Decision, plan_owned_error_declaration,
    plan_owned_error_return_lift,
};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

#[test]
fn generated_error_return_lift_preserves_early_and_tail_successes() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_return")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    build(fixture.path())?;
    assert_eq!(execute(fixture.path(), &[])?.stdout, b"8\n");
    assert_eq!(execute(fixture.path(), &["early"])?.stdout, b"11\n");
    let ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let ErrorDeclarationDecision::Ready(declaration) =
        plan_owned_error_declaration(&ir, node, 0).into_diagnostic()?
    else {
        miette::bail!("missing declaration plan");
    };
    let Decision::Ready(lift) =
        plan_owned_error_return_lift(&ir, node, &declaration).into_diagnostic()?
    else {
        miette::bail!("missing success lift");
    };
    let lifted = lift.preview(&original).into_diagnostic()?;
    assert!(lifted.contains("::core::result::Result<u32, CordialLeafSite0Error>"));
    assert!(
        lifted.contains("operation(fail).unwrap()"),
        "failure policy is a separate operation"
    );
    let appended = declaration.plan().preview(&original).into_diagnostic()?;
    let output = format!("{lifted}{}", &appended[original.len()..]);
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    fs::write(&path, output).into_diagnostic()?;
    build(fixture.path())?;
    assert_eq!(execute(fixture.path(), &[])?.stdout, b"Ok(8)\n");
    assert_eq!(execute(fixture.path(), &["early"])?.stdout, b"Ok(11)\n");
    assert!(!execute(fixture.path(), &["fail"])?.status.success());
    let reloaded = discover(fixture.path())?;
    let node = reloaded
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    assert_eq!(
        plan_owned_error_return_lift(&reloaded, node, &declaration).into_diagnostic()?,
        Decision::Refused(Blocker::MismatchedErrorContract)
    );
    Ok(())
}

#[test]
fn declaration_contracts_cannot_cross_snapshots_modules_or_fixed_signatures() -> miette::Result<()>
{
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_return")?;
    fs::write(
        fixture.path().join("src/other.rs"),
        "fn other() -> u32 { 1 }\n",
    )
    .into_diagnostic()?;
    let mut ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let ErrorDeclarationDecision::Ready(declaration) =
        plan_owned_error_declaration(&ir, node, 0).into_diagnostic()?
    else {
        miette::bail!("missing declaration plan");
    };
    let other = ir
        .node_by_path("other::other")
        .ok_or_else(|| miette::miette!("missing other"))?;
    assert_eq!(
        plan_owned_error_return_lift(&ir, other, &declaration).into_diagnostic()?,
        Decision::Refused(Blocker::MismatchedErrorContract)
    );
    let source = ir
        .node_by_path("operation")
        .ok_or_else(|| miette::miette!("missing operation"))?;
    assert_eq!(
        plan_owned_error_return_lift(&ir, source, &declaration).into_diagnostic()?,
        Decision::Refused(Blocker::AlreadyFallible)
    );
    let facts = serde_json::to_value(function_facts(&ir, "leaf")?).into_diagnostic()?;
    let mut fixed = facts.clone();
    fixed["return_flow"]["boundaries"] = serde_json::json!([FunctionBoundary::Visible]);
    ir.set_attr(node, ATTR_FUNCTION_SOURCE_SYNTAX, fixed)
        .into_diagnostic()?;
    assert_eq!(
        plan_owned_error_return_lift(&ir, node, &declaration).into_diagnostic()?,
        Decision::Refused(Blocker::Boundary(FunctionBoundary::Visible))
    );
    let mut stale = facts;
    stale["source_digest"] = serde_json::json!(vec![0; 32]);
    ir.set_attr(node, ATTR_FUNCTION_SOURCE_SYNTAX, stale)
        .into_diagnostic()?;
    assert_eq!(
        plan_owned_error_return_lift(&ir, node, &declaration).into_diagnostic()?,
        Decision::Refused(Blocker::MismatchedErrorContract)
    );
    Ok(())
}
