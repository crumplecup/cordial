#![cfg(feature = "panics")]

use std::fs;

use cordial::{
    ATTR_FUNCTION_SOURCE_SYNTAX, FunctionBoundary, IrView, ReturnLiftBlocker, ReturnLiftDecision,
    plan_infallible_return_lift,
};
use miette::IntoDiagnostic;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

#[test]
fn return_lift_preview_compiles_executes_and_repeats_without_nesting() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_return_lifting")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("value")
        .ok_or_else(|| miette::miette!("missing value"))?;
    let ReturnLiftDecision::Ready(plan) =
        plan_infallible_return_lift(&ir, node).into_diagnostic()?
    else {
        miette::bail!("isolated return operation must be supported");
    };
    assert_eq!(plan.function(), node);
    assert_eq!(plan.path(), &path);
    let preview = plan.preview(&original).into_diagnostic()?;
    assert_eq!(preview, plan.preview(&original).into_diagnostic()?);
    assert_eq!(
        fs::read_to_string(&path).into_diagnostic()?,
        original,
        "preview must not write"
    );
    let body = function_facts(&ir, "value")?;
    assert_eq!(
        &preview[..body.signature_range().start()],
        &original[..body.signature_range().start()]
    );
    assert!(
        preview.ends_with(&original[body.body_range().end()..]),
        "caller remains an outstanding obligation"
    );
    fs::write(&path, &preview).into_diagnostic()?;
    build(fixture.path())?;
    for (arguments, expected) in [
        (&[][..], b"Ok(11)\n".as_slice()),
        (&["early"][..], b"Ok(7)\n"),
    ] {
        let output = execute(fixture.path(), arguments)?;
        assert!(output.status.success());
        assert_eq!(output.stdout, expected);
    }
    let transformed = discover(fixture.path())?;
    let node = transformed
        .node_by_path("value")
        .ok_or_else(|| miette::miette!("missing lifted value"))?;
    assert_eq!(
        plan_infallible_return_lift(&transformed, node).into_diagnostic()?,
        ReturnLiftDecision::AlreadyLifted
    );
    assert!(
        plan.preview(&preview).is_err(),
        "old plan must reject changed bytes"
    );
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, preview);
    Ok(())
}

#[test]
fn return_lift_refuses_boundaries_and_missing_facts_without_partial_edits() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_return_scopes")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read(&path).into_diagnostic()?;
    let ir = discover(fixture.path())?;
    for (name, boundary) in [
        ("scoped", FunctionBoundary::NestedScope),
        ("fixed_contract", FunctionBoundary::Visible),
        ("residual", FunctionBoundary::Try),
        ("macro_value", FunctionBoundary::Macro),
        ("loop_value", FunctionBoundary::LoopControl),
        ("diverge", FunctionBoundary::NeverOutput),
        ("identity", FunctionBoundary::Generic),
        ("attributed", FunctionBoundary::Attribute),
    ] {
        let node = ir
            .node_by_path(name)
            .ok_or_else(|| miette::miette!("missing {name}"))?;
        assert_eq!(
            plan_infallible_return_lift(&ir, node).into_diagnostic()?,
            ReturnLiftDecision::Refused(ReturnLiftBlocker::Boundary(boundary))
        );
    }
    assert_eq!(
        plan_infallible_return_lift(&ir, ir.root()).into_diagnostic()?,
        ReturnLiftDecision::Refused(ReturnLiftBlocker::MissingFunctionEvidence)
    );
    assert_eq!(fs::read(path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn return_lift_rejects_stale_bytes_and_corrupt_ranges() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_return_lifting")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let mut ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("value")
        .ok_or_else(|| miette::miette!("missing value"))?;
    let ReturnLiftDecision::Ready(plan) =
        plan_infallible_return_lift(&ir, node).into_diagnostic()?
    else {
        miette::bail!("expected return plan");
    };
    assert!(plan.preview(&format!("{original}\n")).is_err());
    let mut facts = serde_json::to_value(function_facts(&ir, "value")?).into_diagnostic()?;
    facts["output_range"] = serde_json::json!({"start": usize::MAX, "end": usize::MAX});
    ir.set_attr(node, ATTR_FUNCTION_SOURCE_SYNTAX, facts)
        .into_diagnostic()?;
    assert_eq!(
        plan_infallible_return_lift(&ir, node).into_diagnostic()?,
        ReturnLiftDecision::Refused(ReturnLiftBlocker::InvalidRanges)
    );
    assert_eq!(fs::read_to_string(path).into_diagnostic()?, original);
    Ok(())
}
