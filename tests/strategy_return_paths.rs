#![cfg(feature = "panics")]

use std::fs;

use cordial::{
    ATTR_FUNCTION_SOURCE_SYNTAX, FunctionSuccessExit, IrView, ReturnLiftBlocker,
    ReturnLiftDecision, plan_infallible_return_lift,
};
use miette::IntoDiagnostic;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

const BASELINE: &str = "branch: 7 11 13\nall_returns: 17 19\nmatching: 23 29 31\nunit: () ()\ntail_unit: () ()\nempty_unit: ()\nunicode: \"é\"\ncounted: 11 112 112\nreturned_branch: 37 41\n";

#[test]
fn structural_return_lifts_compile_and_preserve_paths_and_evaluation_counts() -> miette::Result<()>
{
    cordial::init_tracing();
    let fixture = copy_specimen("apply_return_paths")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    build(fixture.path())?;
    let baseline = execute(fixture.path(), &[])?;
    assert!(baseline.status.success());
    assert_eq!(baseline.stdout, BASELINE.as_bytes());
    let ir = discover(fixture.path())?;
    for (name, expected) in [
        ("branch", "branch: Ok(7) Ok(11) Ok(13)"),
        ("all_returns", "all_returns: Ok(17) Ok(19)"),
        ("matching", "matching: Ok(23) Ok(29) Ok(31)"),
        ("unit", "unit: Ok(()) Ok(())"),
        ("tail_unit", "tail_unit: Ok(()) Ok(())"),
        ("empty_unit", "empty_unit: Ok(())"),
        ("unicode", "unicode: Ok(\"é\")"),
        ("counted", "counted: Ok(11) Ok(112) 112"),
        ("returned_branch", "returned_branch: Ok(37) Ok(41)"),
    ] {
        let node = ir
            .node_by_path(name)
            .ok_or_else(|| miette::miette!("missing {name}"))?;
        let ReturnLiftDecision::Ready(plan) =
            plan_infallible_return_lift(&ir, node).into_diagnostic()?
        else {
            miette::bail!("structural return operation refused for {name}");
        };
        let preview = plan.preview(&original).into_diagnostic()?;
        assert_eq!(preview, plan.preview(&original).into_diagnostic()?);
        fs::write(&path, preview).into_diagnostic()?;
        build(fixture.path())?;
        let output = execute(fixture.path(), &[])?;
        assert!(output.status.success(), "{name}");
        let expected_output: String = BASELINE
            .lines()
            .map(|line| {
                let line = if line.starts_with(&format!("{name}:")) {
                    expected
                } else {
                    line
                };
                format!("{line}\n")
            })
            .collect();
        assert_eq!(output.stdout, expected_output.as_bytes(), "{name}");
        let transformed = discover(fixture.path())?;
        let transformed_node = transformed
            .node_by_path(name)
            .ok_or_else(|| miette::miette!("missing transformed {name}"))?;
        assert_eq!(
            plan_infallible_return_lift(&transformed, transformed_node).into_diagnostic()?,
            ReturnLiftDecision::AlreadyLifted,
            "{name}"
        );
    }
    // Each operation was tested independently against the same original IR.
    fs::write(&path, &original).into_diagnostic()?;
    Ok(())
}

#[test]
fn shared_ir_retains_structural_success_exits_without_nested_wrappers() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_return_paths")?;
    let source = fs::read_to_string(fixture.path().join("src/main.rs")).into_diagnostic()?;
    let ir = discover(fixture.path())?;
    for (name, expressions) in [
        ("branch", vec!["7", "11", "13"]),
        ("all_returns", vec!["17", "19"]),
        ("matching", vec!["23", "value", "31"]),
        ("counted", vec!["*counter", "*counter"]),
        ("returned_branch", vec!["41", "37"]),
    ] {
        let facts = function_facts(&ir, name)?;
        let values: Vec<_> = facts
            .return_flow()
            .success_exits()
            .iter()
            .map(|exit| -> miette::Result<&str> {
                let FunctionSuccessExit::Expression(range) = exit else {
                    miette::bail!("{name} must have only value exits");
                };
                Ok(&source[range.start()..range.end()])
            })
            .collect::<miette::Result<_>>()?;
        assert_eq!(values, expressions, "{name}");
    }
    let unit = function_facts(&ir, "unit")?;
    assert!(matches!(
        unit.return_flow().success_exits().as_slice(),
        [
            FunctionSuccessExit::BareReturn { .. },
            FunctionSuccessExit::BlockUnit { .. }
        ]
    ));
    let tail = function_facts(&ir, "tail_unit")?;
    assert!(matches!(
        tail.return_flow().success_exits().as_slice(),
        [
            FunctionSuccessExit::BareReturn { .. },
            FunctionSuccessExit::MissingElse { .. }
        ]
    ));
    Ok(())
}

#[test]
fn success_exit_evidence_rejects_missing_duplicate_and_invalid_utf8_positions() -> miette::Result<()>
{
    cordial::init_tracing();
    let fixture = copy_specimen("apply_return_paths")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let mut ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("unicode")
        .ok_or_else(|| miette::miette!("missing unicode"))?;
    let facts = serde_json::to_value(function_facts(&ir, "unicode")?).into_diagnostic()?;
    let mut missing = facts.clone();
    missing["return_flow"]
        .as_object_mut()
        .ok_or_else(|| miette::miette!("expected object"))?
        .remove("success_exits");
    ir.set_attr(node, ATTR_FUNCTION_SOURCE_SYNTAX, missing)
        .into_diagnostic()?;
    assert!(plan_infallible_return_lift(&ir, node).is_err());
    let mut duplicate = facts.clone();
    duplicate["return_flow"]["success_exits"] = serde_json::json!([
        facts["return_flow"]["success_exits"][0],
        facts["return_flow"]["success_exits"][0]
    ]);
    ir.set_attr(node, ATTR_FUNCTION_SOURCE_SYNTAX, duplicate)
        .into_diagnostic()?;
    assert_eq!(
        plan_infallible_return_lift(&ir, node).into_diagnostic()?,
        ReturnLiftDecision::Refused(ReturnLiftBlocker::InvalidRanges)
    );
    let offset = original
        .find('é')
        .ok_or_else(|| miette::miette!("missing unicode value"))?;
    let mut invalid = facts;
    invalid["return_flow"]["success_exits"] = serde_json::json!([
        {"Expression": {"start": offset + 1, "end": offset + 2}}
    ]);
    ir.set_attr(node, ATTR_FUNCTION_SOURCE_SYNTAX, invalid)
        .into_diagnostic()?;
    let ReturnLiftDecision::Ready(plan) =
        plan_infallible_return_lift(&ir, node).into_diagnostic()?
    else {
        miette::bail!("numeric ranges remain inside the body");
    };
    assert!(
        plan.preview(&original).is_err(),
        "UTF-8 boundary must be validated before rendering"
    );
    assert_eq!(fs::read_to_string(path).into_diagnostic()?, original);
    Ok(())
}
