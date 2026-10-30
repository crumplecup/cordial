#![cfg(feature = "panics")]

use cordial::{
    ATTR_SOURCE_MODULE_SCOPE, EdgeKind, ErrorDeclarationBlocker as Blocker,
    ErrorDeclarationDecision as Decision, FailurePropagationBlocker, IrView,
    plan_owned_error_declaration,
};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

#[test]
fn generated_chain_compiles_and_preserves_source_payloads_and_caller_location() -> miette::Result<()>
{
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_declaration")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    build(fixture.path())?;
    let baseline = execute(fixture.path(), &[])?;
    assert!(baseline.status.success());
    assert_eq!(baseline.stdout, b"8\n");
    assert!(!execute(fixture.path(), &["fail"])?.status.success());
    let ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let Decision::Ready(declaration) =
        plan_owned_error_declaration(&ir, node, 0).into_diagnostic()?
    else {
        miette::bail!("resolved I/O source must produce a declaration preview");
    };
    assert_eq!(declaration.names().parent(), "CordialLeafSite0Error");
    let plan = declaration.plan();
    assert_eq!(plan.function(), node);
    assert_eq!(plan.path(), &path);
    assert_eq!(
        plan.source_digest(),
        function_facts(&ir, "leaf")?.source_digest()
    );
    let preview = plan.preview(&original).into_diagnostic()?;
    assert!(preview.starts_with(&original));
    assert_eq!(preview, plan.preview(&original).into_diagnostic()?);
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    assert!(plan.preview(&format!("{original}\n")).is_err());
    // Supply the surrounding conversion consumer in isolation. The planner
    // itself has not changed a return signature or a single existing function.
    let consumer = include_str!("fixtures/strategy_generated_error_consumer.rs.txt")
        .replace("$PARENT", declaration.names().parent())
        .replace("$SOURCE", declaration.names().source());
    let main_start = original
        .find("fn main()")
        .ok_or_else(|| miette::miette!("missing main"))?;
    let output = format!(
        "{}\n{consumer}\n{}",
        &original[..main_start],
        &preview[original.len()..]
    );
    fs::write(&path, &output).into_diagnostic()?;
    build(fixture.path())?;
    let success = execute(fixture.path(), &[])?;
    assert!(success.status.success());
    assert_eq!(success.stdout, baseline.stdout);
    let failure = execute(fixture.path(), &["fail"])?;
    assert!(
        failure.status.success(),
        "{}",
        String::from_utf8_lossy(&failure.stderr)
    );
    assert_eq!(
        failure.stdout,
        b"typed source and caller location preserved\n"
    );
    assert!(failure.stderr.is_empty());
    // A fresh collection sees generated declarations and refuses duplication.
    let reloaded = discover(fixture.path())?;
    let node = reloaded
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    assert!(matches!(
        plan_owned_error_declaration(&reloaded, node, 0).into_diagnostic()?,
        Decision::Refused(_)
    ));
    Ok(())
}

#[test]
fn all_generated_names_are_reserved_even_when_used_as_raw_local_identifiers() -> miette::Result<()>
{
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_declaration")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    for name in [
        "CordialLeafSite0Error",
        "CordialLeafSite0ErrorKind",
        "CordialLeafSite0ErrorSource",
    ] {
        let input = original.replace(
            "    operation(fail).unwrap()",
            &format!("    let r#{name} = ();\n    operation(fail).unwrap()"),
        );
        fs::write(&path, &input).into_diagnostic()?;
        let ir = discover(fixture.path())?;
        let node = ir
            .node_by_path("leaf")
            .ok_or_else(|| miette::miette!("missing leaf"))?;
        assert_eq!(
            plan_owned_error_declaration(&ir, node, 0).into_diagnostic()?,
            Decision::Refused(Blocker::NameCollision(name.to_owned()))
        );
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, input);
    }
    Ok(())
}

#[test]
fn unsupported_sources_and_missing_candidates_are_explicit() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_declaration")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    // String is a legal Result error but does not implement Error. A standard
    // absolute path alone must never authorize a generated source wrapper.
    fs::write(
        &path,
        original
            .replace("::std::io::Error>", "::std::string::String>")
            .replace(
                "::std::io::Error::from(::std::io::ErrorKind::PermissionDenied)",
                "::std::string::String::from(\"failure\")",
            ),
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    assert_eq!(
        plan_owned_error_declaration(&ir, node, 0).into_diagnostic()?,
        Decision::Refused(Blocker::UnsupportedSourceType)
    );
    assert_eq!(
        plan_owned_error_declaration(&ir, node, 1).into_diagnostic()?,
        Decision::NoCandidate
    );
    assert_eq!(
        plan_owned_error_declaration(&ir, ir.root(), 0).into_diagnostic()?,
        Decision::Refused(Blocker::FailureEvidence(
            FailurePropagationBlocker::MissingFunctionEvidence
        ))
    );
    Ok(())
}

#[test]
fn corrupt_module_facts_never_authorize_an_insertion() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_declaration")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let mut ir = discover(fixture.path())?;
    let node = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let module = ir.parents(node, EdgeKind::Contains)[0];
    let facts = serde_json::to_value(ir.source_module_scope(module).into_diagnostic()?)
        .into_diagnostic()?;
    for (key, value) in [
        ("insertion_offset", serde_json::json!(0)),
        ("source_digest", serde_json::json!(vec![0; 32])),
    ] {
        let mut bad = facts.clone();
        bad[key] = value;
        ir.set_attr(module, ATTR_SOURCE_MODULE_SCOPE, bad)
            .into_diagnostic()?;
        assert_eq!(
            plan_owned_error_declaration(&ir, node, 0).into_diagnostic()?,
            Decision::Refused(Blocker::InvalidModuleEvidence)
        );
    }
    ir.set_attr(module, ATTR_SOURCE_MODULE_SCOPE, serde_json::json!({}))
        .into_diagnostic()?;
    assert!(plan_owned_error_declaration(&ir, node, 0).is_err());
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    Ok(())
}
