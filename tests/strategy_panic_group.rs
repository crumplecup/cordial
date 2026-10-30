#![cfg(feature = "panics")]

use cordial::{
    ATTR_FUNCTION_CALL_FLOW, ClosedPanicDecision, IrView, PanicComponentBlocker,
    PanicComponentDecision, PanicSiteSelection, apply_closed_panic_migration,
    plan_closed_owned_panic_group, plan_owned_panic_group,
};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

#[test]
fn joint_migration_preserves_order_and_short_circuits_each_failure() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_multiple")?;
    let path = root.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    build(root.path())?;
    let success = execute(root.path(), &[])?;
    assert!(success.status.success());
    assert_eq!(success.stdout, b"1\n2\n3\n1\n2\nsuccess\n");
    let mut failures = Vec::new();
    for name in ["first", "second", "third"] {
        let output = execute(root.path(), &[name])?;
        assert!(!output.status.success());
        failures.push((name, output.stdout));
    }
    let ir = discover(root.path())?;
    assert_eq!(function_facts(&ir, "pair")?.unwraps().len(), 2);
    let (sites, main) = selections(&ir)?;
    let decision = plan_closed_owned_panic_group(&ir, &sites, main).into_diagnostic()?;
    let ClosedPanicDecision::Ready(migration) = decision else {
        miette::bail!("group refused: {decision:?}");
    };
    let mut reversed = sites.clone();
    reversed.reverse();
    assert_eq!(
        plan_closed_owned_panic_group(&ir, &reversed, main).into_diagnostic()?,
        ClosedPanicDecision::Ready(migration.clone())
    );
    assert_eq!(migration.component().sites().len(), 3);
    assert_eq!(migration.component().functions().len(), 5);
    let preview = migration
        .component()
        .plan()
        .preview(&original)
        .into_diagnostic()?;
    assert!(!preview.contains(".unwrap()"));
    assert!(preview.contains("preserve later receiver"));
    assert_eq!(
        preview
            .matches("impl ::core::convert::From<::std::io::Error>")
            .count(),
        1
    );
    let dry = apply_closed_panic_migration(&migration, true).into_diagnostic()?;
    assert_eq!(dry.converted_sites(), 3);
    assert_eq!(dry.written_files(), 0);
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    let applied = apply_closed_panic_migration(&migration, false).into_diagnostic()?;
    assert_eq!(applied.converted_sites(), 3);
    assert_eq!(applied.changed_functions(), 5);
    assert_eq!(applied.written_files(), 1);
    build(root.path())?;
    let after = execute(root.path(), &[])?;
    assert!(after.status.success());
    assert_eq!(after.stdout, success.stdout);
    for (name, prefix) in failures {
        let after = execute(root.path(), &[name])?;
        assert_eq!(after.status.code(), Some(1));
        assert_eq!(after.stdout, prefix, "no continuation after {name}");
        let error = String::from_utf8_lossy(&after.stderr);
        assert!(error.contains("PermissionDenied"), "{error}");
        assert!(!error.contains("panicked"), "{error}");
    }
    assert!(apply_closed_panic_migration(&migration, false).is_err());
    Ok(())
}

#[test]
fn incomplete_duplicate_or_incompatible_groups_expose_no_partial_plan() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_multiple")?;
    let path = root.path().join("src/main.rs");
    let original = fs::read(&path).into_diagnostic()?;
    let mut ir = discover(root.path())?;
    let (sites, main) = selections(&ir)?;
    assert!(matches!(
        plan_owned_panic_group(&ir, &sites[1..], main).into_diagnostic()?,
        PanicComponentDecision::Refused(PanicComponentBlocker::AdditionalPanicSites(_))
    ));
    let mut duplicate = sites.clone();
    duplicate.push(sites[0]);
    assert_eq!(
        plan_owned_panic_group(&ir, &duplicate, main).into_diagnostic()?,
        PanicComponentDecision::Refused(PanicComponentBlocker::InvalidEvidence)
    );
    assert_eq!(
        plan_owned_panic_group(&ir, &[], main).into_diagnostic()?,
        PanicComponentDecision::NoCandidate
    );
    // Break just the second leaf's resolution in the shared IR. The first
    // valid leaf must not escape as a partial preview or writable token.
    let other = sites[2].function();
    let mut flow =
        serde_json::to_value(ir.function_call_flow(other).into_diagnostic()?).into_diagnostic()?;
    flow["calls"] = serde_json::json!([]);
    ir.set_attr(other, ATTR_FUNCTION_CALL_FLOW, flow)
        .into_diagnostic()?;
    assert!(matches!(
        plan_closed_owned_panic_group(&ir, &sites, main).into_diagnostic()?,
        ClosedPanicDecision::Refused(_)
    ));
    assert_eq!(fs::read(&path).into_diagnostic()?, original);
    Ok(())
}

fn selections(ir: &dyn IrView) -> miette::Result<(Vec<PanicSiteSelection>, cordial::NodeId)> {
    let lookup = |name| {
        ir.node_by_path(name)
            .ok_or_else(|| miette::miette!("missing {name}"))
    };
    let pair = lookup("pair")?;
    Ok((
        vec![
            PanicSiteSelection::new(pair, 0),
            PanicSiteSelection::new(pair, 1),
            PanicSiteSelection::new(lookup("other")?, 0),
        ],
        lookup("main")?,
    ))
}
