#![cfg(feature = "panics")]

use cordial::{
    ATTR_FUNCTION_CALL_FLOW, ATTR_FUNCTION_SOURCE_SYNTAX, FunctionBoundary, IrView,
    PanicComponentBlocker as Blocker, PanicComponentDecision as Decision, ReturnLiftBlocker,
    plan_owned_panic_component,
};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

#[test]
fn fresh_panic_migration_propagates_through_diamonds_repetition_and_recursion() -> miette::Result<()>
{
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_component")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    build(fixture.path())?;
    assert_eq!(execute(fixture.path(), &[])?.stdout, b"24 3\n");
    assert_eq!(execute(fixture.path(), &["deep"])?.stdout, b"25 3\n");
    assert!(!execute(fixture.path(), &["fail"])?.status.success());
    let ir = discover(fixture.path())?;
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let boundary = ir
        .node_by_path("entry")
        .ok_or_else(|| miette::miette!("missing entry"))?;
    let Decision::Ready(component) =
        plan_owned_panic_component(&ir, leaf, 0, boundary).into_diagnostic()?
    else {
        miette::bail!("component was not planned");
    };
    assert_eq!(component.boundary(), boundary);
    assert_eq!(component.functions().len(), 7);
    for name in [
        "leaf",
        "left",
        "right",
        "middle",
        "recursive",
        "recursive_other",
        "entry",
    ] {
        assert!(
            component.functions().contains(
                &ir.node_by_path(name)
                    .ok_or_else(|| miette::miette!("missing {name}"))?
            )
        );
    }
    assert!(
        !component.functions().contains(
            &ir.node_by_path("operation")
                .ok_or_else(|| miette::miette!("missing operation"))?
        )
    );
    let output = component.plan().preview(&original).into_diagnostic()?;
    assert!(output.contains("preserve receiver comment"));
    assert!(!output.contains(".unwrap()"));
    assert_eq!(output.matches("enum CordialLeafSite0ErrorKind").count(), 1);
    assert_eq!(
        output
            .matches("::core::result::Result<u32, CordialLeafSite0Error>")
            .count(),
        7
    );
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    assert!(
        component
            .plan()
            .preview(&(original.clone() + "\n"))
            .is_err()
    );
    assert_eq!(
        plan_owned_panic_component(&ir, leaf, 0, boundary).into_diagnostic()?,
        Decision::Ready(component.clone())
    );
    fs::write(&path, output).into_diagnostic()?;
    build(fixture.path())?;
    assert_eq!(execute(fixture.path(), &[])?.stdout, b"Ok(24) 3\n");
    assert_eq!(execute(fixture.path(), &["deep"])?.stdout, b"Ok(25) 3\n");
    let failure = execute(fixture.path(), &["fail"])?;
    assert!(failure.status.success(), "failure must return, not panic");
    let failure = String::from_utf8(failure.stdout).into_diagnostic()?;
    assert!(failure.starts_with("Err(CordialLeafSite0Error"));
    assert!(failure.contains("PermissionDenied"));
    assert!(
        failure.ends_with(" 1\n"),
        "failure must skip every downstream success continuation: {failure}"
    );
    let reloaded = discover(fixture.path())?;
    let leaf = reloaded
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let boundary = reloaded
        .node_by_path("entry")
        .ok_or_else(|| miette::miette!("missing entry"))?;
    assert_eq!(
        plan_owned_panic_component(&reloaded, leaf, 0, boundary).into_diagnostic()?,
        Decision::NoCandidate
    );
    Ok(())
}

#[test]
fn a_refused_transitive_caller_prevents_the_entire_component_preview() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_component")?;
    let mut ir = discover(fixture.path())?;
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let boundary = ir
        .node_by_path("entry")
        .ok_or_else(|| miette::miette!("missing entry"))?;
    let middle = ir
        .node_by_path("middle")
        .ok_or_else(|| miette::miette!("missing middle"))?;
    let facts = serde_json::to_value(function_facts(&ir, "middle")?).into_diagnostic()?;
    let mut fixed = facts.clone();
    fixed["return_flow"]["boundaries"] = serde_json::json!([FunctionBoundary::Visible]);
    ir.set_attr(middle, ATTR_FUNCTION_SOURCE_SYNTAX, fixed)
        .into_diagnostic()?;
    assert_eq!(
        plan_owned_panic_component(&ir, leaf, 0, boundary).into_diagnostic()?,
        Decision::Refused(Blocker::ReturnContract {
            function: middle,
            reason: ReturnLiftBlocker::Boundary(FunctionBoundary::Visible)
        })
    );
    ir.set_attr(middle, ATTR_FUNCTION_SOURCE_SYNTAX, facts)
        .into_diagnostic()?;
    let mut flow =
        serde_json::to_value(ir.function_call_flow(middle).into_diagnostic()?).into_diagnostic()?;
    flow["calls"] = serde_json::json!([]);
    ir.set_attr(middle, ATTR_FUNCTION_CALL_FLOW, flow)
        .into_diagnostic()?;
    assert!(
        matches!(plan_owned_panic_component(&ir, leaf, 0, boundary).into_diagnostic()?, Decision::Refused(Blocker::CallEvidence { function, .. }) if function == middle)
    );
    Ok(())
}
