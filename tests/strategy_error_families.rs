#![cfg(feature = "panics")]
//! Family evidence is derived in the production pipeline and persisted in IR.

#[path = "support/strategy_specimens.rs"]
mod specimens;

use cordial::{
    ATTR_FUNCTION_ERROR_FAMILY, ATTR_FUNCTION_SOURCE_SYNTAX, ErrorDeclarationBlocker,
    ErrorDeclarationDecision, ErrorFamilyBasis, ErrorFamilyGapReason, FailurePropagationBlocker,
    FunctionErrorFamily, IrView, PanicSiteSelection, SourceCallUse,
    plan_owned_error_group_declaration,
};
use miette::IntoDiagnostic;
use specimens::{build, copy_specimen, discover, execute, function_facts};
use std::fs;

fn family(ir: &dyn IrView, name: &str) -> miette::Result<FunctionErrorFamily> {
    let id = ir
        .node_by_path(name)
        .ok_or_else(|| miette::miette!("missing {name}"))?;
    ir.function_error_family(id)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing family {name}"))
}

fn identities(family: &FunctionErrorFamily) -> Vec<&str> {
    family
        .members()
        .iter()
        .map(|member| member.identity().as_str())
        .collect()
}

#[test]
fn chains_diamonds_recursion_and_handlers_retain_their_own_contracts() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    fs::write(
        fixture.path().join("src/main.rs"),
        include_str!("fixtures/strategy_error_families.rs.txt"),
    )
    .into_diagnostic()?;
    build(fixture.path())?;
    assert!(execute(fixture.path(), &[])?.status.success());
    let ir = discover(fixture.path())?;
    for name in [
        "read",
        "number",
        "pair",
        "left",
        "right",
        "diamond",
        "recursive_a",
        "recursive_b",
        "empty_recursion",
        "handled",
        "handled_bool",
        "argument",
        "distinct_origins",
        "infallible_use",
    ] {
        let result = family(&ir, name)?;
        assert_eq!(result.basis(), ErrorFamilyBasis::PanicMigration);
        assert_eq!(
            result.source_digest(),
            function_facts(&ir, name)?.source_digest()
        );
        assert!(result.gaps().is_empty(), "{name}: {:?}", result.gaps());
        let expected = match name {
            "read" | "distinct_origins" => vec!["::std::io::Error"],
            "number" | "argument" => vec!["::std::num::ParseIntError"],
            "empty_recursion" | "handled" | "handled_bool" | "infallible_use" => vec![],
            _ => vec!["::std::io::Error", "::std::num::ParseIntError"],
        };
        assert_eq!(identities(&result), expected, "{name}");
    }
    let io = ir.node_by_path("io").ok_or_else(|| miette::miette!("io"))?;
    for name in ["read", "pair", "diamond", "recursive_b"] {
        assert_eq!(family(&ir, name)?.members()[0].origins(), &[io]);
    }
    assert_eq!(
        family(&ir, "distinct_origins")?.members()[0]
            .origins()
            .len(),
        2
    );
    for name in ["io", "parse", "residual"] {
        assert_eq!(family(&ir, name)?.basis(), ErrorFamilyBasis::DeclaredResult);
        assert!(family(&ir, name)?.gaps().is_empty());
    }
    assert!(family(&ir, "infallible")?.members().is_empty());
    let calls = function_facts(&ir, "argument")?;
    assert_eq!(
        calls.calls()[0].usage(),
        &Some(SourceCallUse::HandledResult)
    );
    assert_eq!(calls.calls()[1].usage(), &Some(SourceCallUse::Value));
    Ok(())
}

#[test]
fn unknown_effects_remain_located_obligations_through_callers_and_cycles() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_failure_site")?;
    fs::write(
        fixture.path().join("src/main.rs"),
        include_str!("fixtures/strategy_error_families.rs.txt"),
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    let unknown = family(&ir, "unknown")?;
    assert_eq!(identities(&unknown), ["::std::io::Error"]);
    assert_eq!(unknown.gaps().len(), 1);
    assert_eq!(
        unknown.gaps()[0].reason(),
        ErrorFamilyGapReason::UnresolvedCall
    );
    assert!(unknown.gaps()[0].range().is_some());
    for name in [
        "unknown_caller",
        "unknown_recursive_a",
        "unknown_recursive_b",
    ] {
        let inherited = family(&ir, name)?;
        assert_eq!(identities(&inherited), identities(&unknown));
        assert_eq!(inherited.gaps(), unknown.gaps(), "{name}");
    }
    for (name, reason) in [
        (
            "unmodeled_method",
            ErrorFamilyGapReason::UnsupportedOperation,
        ),
        (
            "discarded_result",
            ErrorFamilyGapReason::UnsupportedResultUse,
        ),
        ("indirect", ErrorFamilyGapReason::UnresolvedCall),
        (
            "unproven_handler",
            ErrorFamilyGapReason::UnprovenResultReceiver,
        ),
    ] {
        assert!(
            family(&ir, name)?
                .gaps()
                .iter()
                .any(|gap| gap.reason() == reason),
            "{name}"
        );
    }
    assert_eq!(
        identities(&family(&ir, "unmodeled_method")?),
        ["::std::io::Error"]
    );
    Ok(())
}

#[test]
fn missing_corrupt_and_stale_families_cannot_authorize_declarations() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_sources")?;
    let ir = discover(fixture.path())?;
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("leaf"))?;
    let io = ir
        .node_by_path("io_operation")
        .ok_or_else(|| miette::miette!("io"))?;
    let selection = [PanicSiteSelection::new(leaf, 0)];
    for (field, value) in [
        ("source_digest", serde_json::json!(vec![0; 32])),
        ("basis", serde_json::json!("PanicMigration")),
        ("members", serde_json::json!([])),
        (
            "members",
            serde_json::json!([{
                "identity": "::std::num::ParseIntError", "origins": [io]
            }]),
        ),
        (
            "members",
            serde_json::json!([{
                "identity": "::std::io::Error", "origins": [leaf]
            }]),
        ),
    ] {
        let mut changed = ir.clone();
        let mut value_family =
            serde_json::to_value(family(&ir, "io_operation")?).into_diagnostic()?;
        value_family[field] = value;
        changed
            .set_attr(io, ATTR_FUNCTION_ERROR_FAMILY, value_family)
            .into_diagnostic()?;
        assert!(matches!(
            plan_owned_error_group_declaration(&changed, &selection).into_diagnostic()?,
            ErrorDeclarationDecision::Refused(_)
        ));
    }
    let mut snapshot = serde_json::to_value(ir.snapshot().into_diagnostic()?).into_diagnostic()?;
    let nodes = snapshot["nodes"]
        .as_array_mut()
        .ok_or_else(|| miette::miette!("nodes"))?;
    for node in nodes {
        node["attrs"]
            .as_array_mut()
            .ok_or_else(|| miette::miette!("attrs"))?
            .retain(|entry| entry[0].as_str() != Some(ATTR_FUNCTION_ERROR_FAMILY));
    }
    let mut missing =
        cordial::CrateIr::from_snapshot(serde_json::from_value(snapshot).into_diagnostic()?)
            .into_diagnostic()?;
    assert!(
        missing
            .function_error_family(io)
            .into_diagnostic()?
            .is_none()
    );
    assert_eq!(
        plan_owned_error_group_declaration(&missing, &selection).into_diagnostic()?,
        ErrorDeclarationDecision::Refused(ErrorDeclarationBlocker::FailureEvidence(
            FailurePropagationBlocker::MissingCallEvidence
        ))
    );
    missing
        .set_attr(
            io,
            ATTR_FUNCTION_ERROR_FAMILY,
            serde_json::json!({"broken": true}),
        )
        .into_diagnostic()?;
    assert!(missing.function_error_family(io).is_err());
    assert!(plan_owned_error_group_declaration(&missing, &selection).is_err());
    // Older call facts remain readable, but do not acquire new usage evidence.
    let mut old = serde_json::to_value(function_facts(&ir, "leaf")?).into_diagnostic()?;
    old["calls"][0]
        .as_object_mut()
        .ok_or_else(|| miette::miette!("call"))?
        .remove("usage");
    missing
        .set_attr(leaf, ATTR_FUNCTION_SOURCE_SYNTAX, old)
        .into_diagnostic()?;
    assert!(
        missing
            .function_source_syntax(leaf)
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("facts"))?
            .calls()[0]
            .usage()
            .is_none()
    );
    Ok(())
}
