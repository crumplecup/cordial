#![cfg(feature = "panics")]

use cordial::{
    ATTR_FUNCTION_CALL_FLOW, ATTR_FUNCTION_SOURCE_SYNTAX, CallResolutionFailure,
    FailurePropagationBlocker as Blocker, FailurePropagationDecision as Decision, IrView,
    plan_result_unwrap_propagation,
};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

#[test]
fn propagation_preserves_values_order_comments_and_the_typed_source() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_result_propagation")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    build(fixture.path())?;
    let baseline = execute(fixture.path(), &[])?;
    assert!(baseline.status.success());
    assert_eq!(
        baseline.stdout,
        b"Ok(8)|[\"before\", \"source\", \"after\"]\n"
    );
    assert!(!execute(fixture.path(), &["fail"])?.status.success());
    let ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let Decision::Ready(plan) = plan_result_unwrap_propagation(&ir, node, 0).into_diagnostic()?
    else {
        miette::bail!("resolved Result must produce a propagation preview");
    };
    assert_eq!(plan.function(), node);
    assert_eq!(plan.path(), &path);
    let preview = plan.preview(&original).into_diagnostic()?;
    assert_eq!(preview, plan.preview(&original).into_diagnostic()?);
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    assert!(plan.preview(&format!("{original}\n")).is_err());
    for comment in [
        "/* receiver */",
        "/* dot */",
        "/* name */",
        "/* arguments */",
    ] {
        assert!(preview.contains(comment));
    }
    let facts = function_facts(&ir, "leaf")?;
    let site = &facts.unwraps()[0];
    assert_eq!(
        &preview[..site.receiver().end()],
        &original[..site.receiver().end()]
    );
    assert!(preview.ends_with(&original[site.range().end()..]));
    fs::write(&path, &preview).into_diagnostic()?;
    build(fixture.path())?;
    let success = execute(fixture.path(), &[])?;
    assert!(success.status.success());
    assert_eq!(success.stdout, baseline.stdout);
    let failure = execute(fixture.path(), &["fail"])?;
    assert!(
        failure.status.success(),
        "the error is returned instead of panicking"
    );
    assert_eq!(
        failure.stdout,
        b"Err(Kind(PermissionDenied))|[\"before\", \"source\"]\n"
    );
    assert!(failure.stderr.is_empty());
    let reloaded = discover(fixture.path())?;
    let node = reloaded
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing reloaded leaf"))?;
    assert_eq!(
        plan_result_unwrap_propagation(&reloaded, node, 0).into_diagnostic()?,
        Decision::NoCandidate
    );
    assert!(plan.preview(&preview).is_err());
    Ok(())
}

#[test]
fn unresolved_receivers_and_error_contracts_never_produce_partial_edits() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_result_propagation")?;
    let path = fixture.path().join("src/main.rs");
    fs::write(&path, include_str!("fixtures/strategy_failure_refusals.rs")).into_diagnostic()?;
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let ir = discover(fixture.path())?;
    for (name, blocker) in [
        ("mismatch", Blocker::ErrorTypeMismatch),
        ("alias", Blocker::UnresolvedErrorType),
        ("relative", Blocker::NotStandardResult),
        ("not_result", Blocker::NotStandardResult),
        (
            "shadowed",
            Blocker::UnresolvedReceiver(CallResolutionFailure::UnsupportedCallee),
        ),
        (
            "parenthesized",
            Blocker::UnresolvedReceiver(CallResolutionFailure::UnsupportedCallee),
        ),
    ] {
        let node = ir
            .node_by_path(name)
            .ok_or_else(|| miette::miette!("missing {name}"))?;
        assert_eq!(
            plan_result_unwrap_propagation(&ir, node, 0).into_diagnostic()?,
            Decision::Refused(blocker),
            "{name}"
        );
    }
    assert_eq!(
        plan_result_unwrap_propagation(&ir, ir.root(), 0).into_diagnostic()?,
        Decision::Refused(Blocker::MissingFunctionEvidence)
    );
    let nested = ir
        .node_by_path("nested")
        .ok_or_else(|| miette::miette!("missing nested"))?;
    assert_eq!(
        plan_result_unwrap_propagation(&ir, nested, 0).into_diagnostic()?,
        Decision::NoCandidate
    );
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn namespace_ambiguity_prevents_standard_path_authority() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let (doc, body) = original
        .split_once('\n')
        .ok_or_else(|| miette::miette!("missing module doc"))?;
    for prefix in ["use std as alternate;", "mod std {}", "mod core {}"] {
        fs::write(&path, format!("{doc}\n{prefix}\n{body}")).into_diagnostic()?;
        let ir = discover(fixture.path())?;
        let node = ir
            .node_by_path("leaf")
            .ok_or_else(|| miette::miette!("missing leaf"))?;
        assert_eq!(
            plan_result_unwrap_propagation(&ir, node, 0).into_diagnostic()?,
            Decision::Refused(Blocker::UnresolvedReceiver(
                CallResolutionFailure::OpaqueScope
            )),
            "{prefix}"
        );
    }
    Ok(())
}

#[test]
fn inconsistent_snapshots_and_token_ranges_are_rejected() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let mut ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let flow =
        serde_json::to_value(ir.function_call_flow(node).into_diagnostic()?).into_diagnostic()?;
    let mut bad_flow = flow.clone();
    let byte = bad_flow["source_digest"][0]
        .as_u64()
        .ok_or_else(|| miette::miette!("missing digest"))?;
    bad_flow["source_digest"][0] = serde_json::json!(255 - byte);
    ir.set_attr(node, ATTR_FUNCTION_CALL_FLOW, bad_flow)
        .into_diagnostic()?;
    assert_eq!(
        plan_result_unwrap_propagation(&ir, node, 0).into_diagnostic()?,
        Decision::Refused(Blocker::InvalidEvidence)
    );
    ir.set_attr(node, ATTR_FUNCTION_CALL_FLOW, flow)
        .into_diagnostic()?;
    let facts = serde_json::to_value(function_facts(&ir, "leaf")?).into_diagnostic()?;
    let mut bad_facts = facts.clone();
    bad_facts["unwraps"][0]["suffix_tokens"][0]["end"] = serde_json::json!(usize::MAX);
    ir.set_attr(node, ATTR_FUNCTION_SOURCE_SYNTAX, bad_facts)
        .into_diagnostic()?;
    assert_eq!(
        plan_result_unwrap_propagation(&ir, node, 0).into_diagnostic()?,
        Decision::Refused(Blocker::InvalidEvidence)
    );
    ir.set_attr(node, ATTR_FUNCTION_SOURCE_SYNTAX, facts)
        .into_diagnostic()?;
    ir.set_attr(node, ATTR_FUNCTION_CALL_FLOW, serde_json::json!({}))
        .into_diagnostic()?;
    assert!(plan_result_unwrap_propagation(&ir, node, 0).is_err());
    assert_eq!(fs::read_to_string(path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn same_spelling_user_method_is_never_rewritten() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_unwrap_lookalike")?;
    build(fixture.path())?;
    assert_eq!(execute(fixture.path(), &[])?.stdout, b"7\n");
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    assert_eq!(
        plan_result_unwrap_propagation(&ir, node, 0).into_diagnostic()?,
        Decision::Refused(Blocker::UnresolvedReceiver(
            CallResolutionFailure::UnsupportedCallee
        ))
    );
    assert_eq!(fs::read_to_string(path).into_diagnostic()?, original);
    Ok(())
}

#[test]
fn repeated_sites_are_planned_independently_after_each_recollection() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    let path = fixture.path().join("src/main.rs");
    fs::write(
        &path,
        include_str!("fixtures/strategy_repeated_propagation.rs"),
    )
    .into_diagnostic()?;
    build(fixture.path())?;
    let baseline = execute(fixture.path(), &[])?.stdout;
    assert_eq!(baseline, b"Ok(14)\n");
    for index in [1, 0] {
        let original = fs::read_to_string(&path).into_diagnostic()?;
        let ir = discover(fixture.path())?;
        let node = ir
            .node_by_path("leaf")
            .ok_or_else(|| miette::miette!("missing leaf"))?;
        let Decision::Ready(plan) =
            plan_result_unwrap_propagation(&ir, node, index).into_diagnostic()?
        else {
            miette::bail!(
                "each site must be supported independently, including after the first question mark"
            );
        };
        fs::write(&path, plan.preview(&original).into_diagnostic()?).into_diagnostic()?;
        build(fixture.path())?;
        assert_eq!(execute(fixture.path(), &[])?.stdout, baseline);
    }
    assert!(
        function_facts(&discover(fixture.path())?, "leaf")?
            .unwraps()
            .is_empty()
    );
    Ok(())
}
