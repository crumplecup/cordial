#![cfg(feature = "panics")]

use cordial::{
    ATTR_FUNCTION_CALL_FLOW, ATTR_FUNCTION_SOURCE_SYNTAX, CallerAdaptationBlocker as Blocker,
    CallerAdaptationDecision as Decision, ErrorDeclarationDecision, FailurePropagationBlocker,
    FunctionBoundary, FunctionEditPlan, IrView, ReturnLiftBlocker, plan_owned_caller_adaptation,
    plan_owned_error_declaration,
};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

#[test]
fn one_caller_adaptation_covers_repeated_early_and_exact_tail_calls() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_caller")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    build(fixture.path())?;
    for (args, expected) in [
        (vec![], "14 2\n"),
        (vec!["early"], "7 1\n"),
        (vec!["tail"], "7 1\n"),
    ] {
        assert_eq!(execute(fixture.path(), &args)?.stdout, expected.as_bytes());
    }
    let ir = discover(fixture.path())?;
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let caller = ir
        .node_by_path("caller")
        .ok_or_else(|| miette::miette!("missing caller"))?;
    let ErrorDeclarationDecision::Ready(declaration) =
        plan_owned_error_declaration(&ir, leaf, 0).into_diagnostic()?
    else {
        miette::bail!("missing error declaration");
    };
    let Decision::Ready(adaptation) =
        plan_owned_caller_adaptation(&ir, caller, 0, &declaration).into_diagnostic()?
    else {
        miette::bail!("missing caller adaptation");
    };
    assert_eq!(adaptation.callee().function(), leaf);
    assert_eq!(adaptation.caller().function(), caller);
    let preview = adaptation.caller().preview(&original).into_diagnostic()?;
    assert_eq!(preview.matches("count)?").count(), 4);
    assert!(preview.contains("::core::result::Result::Ok(leaf(fail, count)?)"));
    // Each isolated plan is previewed against its original snapshot. The test
    // assembles their disjoint edits; production component composition follows.
    let output = assemble(
        &original,
        &[adaptation.caller(), adaptation.callee(), declaration.plan()],
    )?;
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    fs::write(&path, output).into_diagnostic()?;
    build(fixture.path())?;
    for (args, expected) in [
        (vec![], "Ok(14) 2\n"),
        (vec!["early"], "Ok(7) 1\n"),
        (vec!["tail"], "Ok(7) 1\n"),
    ] {
        assert_eq!(execute(fixture.path(), &args)?.stdout, expected.as_bytes());
    }
    assert!(!execute(fixture.path(), &["fail"])?.status.success());
    Ok(())
}

#[test]
fn caller_adaptation_rejects_fixed_signatures_unresolved_and_stale_calls() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_caller")?;
    let mut ir = discover(fixture.path())?;
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let caller = ir
        .node_by_path("caller")
        .ok_or_else(|| miette::miette!("missing caller"))?;
    let ErrorDeclarationDecision::Ready(declaration) =
        plan_owned_error_declaration(&ir, leaf, 0).into_diagnostic()?
    else {
        miette::bail!("missing error declaration");
    };
    assert_eq!(
        plan_owned_caller_adaptation(&ir, caller, usize::MAX, &declaration).into_diagnostic()?,
        Decision::NoCandidate
    );
    let facts = serde_json::to_value(function_facts(&ir, "caller")?).into_diagnostic()?;
    let mut fixed = facts.clone();
    fixed["return_flow"]["boundaries"] = serde_json::json!([FunctionBoundary::Visible]);
    ir.set_attr(caller, ATTR_FUNCTION_SOURCE_SYNTAX, fixed)
        .into_diagnostic()?;
    assert_eq!(
        plan_owned_caller_adaptation(&ir, caller, 0, &declaration).into_diagnostic()?,
        Decision::Refused(Blocker::ReturnContract(ReturnLiftBlocker::Boundary(
            FunctionBoundary::Visible
        )))
    );
    ir.set_attr(caller, ATTR_FUNCTION_SOURCE_SYNTAX, facts)
        .into_diagnostic()?;
    let flow =
        serde_json::to_value(ir.function_call_flow(caller).into_diagnostic()?).into_diagnostic()?;
    let mut stale = flow.clone();
    stale["calls"][3]["resolution"]["Source"]["source_digest"] = serde_json::json!(vec![0; 32]);
    ir.set_attr(caller, ATTR_FUNCTION_CALL_FLOW, stale)
        .into_diagnostic()?;
    assert_eq!(
        plan_owned_caller_adaptation(&ir, caller, 0, &declaration).into_diagnostic()?,
        Decision::Refused(Blocker::CallEvidence(
            FailurePropagationBlocker::InvalidEvidence
        ))
    );
    let mut unresolved = flow;
    unresolved["calls"][0]["resolution"] = serde_json::json!({"Unresolved": "UnsupportedCallee"});
    ir.set_attr(caller, ATTR_FUNCTION_CALL_FLOW, unresolved)
        .into_diagnostic()?;
    assert!(matches!(
        plan_owned_caller_adaptation(&ir, caller, 0, &declaration).into_diagnostic()?,
        Decision::Refused(Blocker::CallEvidence(
            FailurePropagationBlocker::UnresolvedReceiver(_)
        ))
    ));
    ir.set_attr(caller, ATTR_FUNCTION_CALL_FLOW, serde_json::json!({}))
        .into_diagnostic()?;
    assert!(plan_owned_caller_adaptation(&ir, caller, 0, &declaration).is_err());
    Ok(())
}

fn assemble(source: &str, plans: &[&FunctionEditPlan]) -> miette::Result<String> {
    let mut edits = Vec::new();
    for plan in plans {
        plan.preview(source).into_diagnostic()?;
        edits.extend(plan.edits());
    }
    edits.sort_by_key(|edit| (edit.range().start(), edit.range().end()));
    let mut output = source.to_owned();
    for edit in edits.into_iter().rev() {
        output.replace_range(edit.range().start()..edit.range().end(), edit.replacement());
    }
    Ok(output)
}
