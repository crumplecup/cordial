#![cfg(feature = "panics")]

use cordial::{
    ClosedPanicDecision, ErrorDeclarationBlocker, ErrorDeclarationDecision, IrView,
    PanicSiteSelection, apply_closed_panic_migration, plan_closed_owned_panic_group,
    plan_owned_error_group_declaration,
};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

#[test]
fn isolated_declarations_preserve_distinct_sources_and_track_caller() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_sources")?;
    let path = root.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let ir = discover(root.path())?;
    assert_eq!(function_facts(&ir, "leaf")?.unwraps().len(), 2);
    let (sites, _) = selections(&ir)?;
    let decision = plan_owned_error_group_declaration(&ir, &sites).into_diagnostic()?;
    let ErrorDeclarationDecision::Ready(declaration) = decision else {
        miette::bail!("declarations refused: {decision:?}");
    };
    let names = declaration.names();
    assert_eq!(names.sources().len(), 2);
    assert_eq!(names.sources()[0].native_type(), "::std::io::Error");
    assert_eq!(
        names.sources()[1].native_type(),
        "::std::num::ParseIntError"
    );
    let mut reversed = sites.clone();
    reversed.reverse();
    assert_eq!(
        plan_owned_error_group_declaration(&ir, &reversed).into_diagnostic()?,
        ErrorDeclarationDecision::Ready(declaration.clone())
    );
    let preview = declaration.plan().preview(&original).into_diagnostic()?;
    assert!(preview.starts_with(&original));
    let consumer = include_str!("fixtures/strategy_native_sources_consumer.rs.txt")
        .replace("$PARENT", names.parent())
        .replace("$IO_SOURCE", names.sources()[0].wrapper())
        .replace("$PARSE_SOURCE", names.sources()[1].wrapper());
    let main = original
        .find("fn main()")
        .ok_or_else(|| miette::miette!("missing main"))?;
    fs::write(
        &path,
        format!(
            "{}\n{consumer}\n{}",
            &original[..main],
            &preview[original.len()..]
        ),
    )
    .into_diagnostic()?;
    build(root.path())?;
    let output = execute(root.path(), &[])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"both typed sources and caller locations preserved\n"
    );
    Ok(())
}

#[test]
fn composed_native_alternatives_compile_and_retain_both_failure_types() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_sources")?;
    build(root.path())?;
    let success = execute(root.path(), &[])?;
    assert!(success.status.success());
    assert_eq!(success.stdout, b"success\n");
    for mode in ["io", "parse"] {
        assert!(!execute(root.path(), &[mode])?.status.success());
    }
    let ir = discover(root.path())?;
    let (sites, main) = selections(&ir)?;
    let decision = plan_closed_owned_panic_group(&ir, &sites, main).into_diagnostic()?;
    let ClosedPanicDecision::Ready(migration) = decision else {
        miette::bail!("group refused: {decision:?}");
    };
    apply_closed_panic_migration(&migration, false).into_diagnostic()?;
    build(root.path())?;
    assert_eq!(execute(root.path(), &[])?.stdout, success.stdout);
    for (mode, variant, native) in [
        ("io", "Io(", "PermissionDenied"),
        ("parse", "ParseInt(", "InvalidDigit"),
    ] {
        let output = execute(root.path(), &[mode])?;
        assert_eq!(output.status.code(), Some(1));
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(variant) && error.contains(native), "{error}");
        assert!(!error.contains("panicked"), "{error}");
    }
    Ok(())
}

#[test]
fn every_wrapper_name_is_reserved_and_core_parse_identity_is_supported() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_sources")?;
    let path = root.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let collision = "CordialLeafSite0ErrorSourceParseInt";
    fs::write(
        &path,
        original.replace(
            "fn leaf(mode: u32) -> u32 {",
            &format!("fn leaf(mode: u32) -> u32 {{\n let r#{collision} = ();"),
        ),
    )
    .into_diagnostic()?;
    let ir = discover(root.path())?;
    let (sites, _) = selections(&ir)?;
    assert_eq!(
        plan_owned_error_group_declaration(&ir, &sites).into_diagnostic()?,
        ErrorDeclarationDecision::Refused(ErrorDeclarationBlocker::NameCollision(
            collision.to_owned()
        ))
    );
    fs::write(
        &path,
        original.replace("::std::num::ParseIntError", "::core::num::ParseIntError"),
    )
    .into_diagnostic()?;
    let ir = discover(root.path())?;
    let (sites, _) = selections(&ir)?;
    let ErrorDeclarationDecision::Ready(declaration) =
        plan_owned_error_group_declaration(&ir, &sites).into_diagnostic()?
    else {
        miette::bail!("core reexport refused");
    };
    assert_eq!(declaration.names().sources().len(), 2);
    assert_eq!(
        declaration.names().sources()[1].native_type(),
        "::std::num::ParseIntError"
    );
    Ok(())
}

#[test]
fn std_and_core_parse_sources_share_one_conversion() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_sources")?;
    let path = root.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let source = format!(
        "{}\nfn parse_core(input: &str) -> ::core::result::Result<u32, ::core::num::ParseIntError> {{ input.parse::<u32>() }}\n",
        original
            .replace(
                "first + parse_operation(input).unwrap()",
                "first + parse_operation(input).unwrap() + parse_core(input).unwrap()"
            )
            .replace("value == 12", "value == 17")
    );
    fs::write(&path, source).into_diagnostic()?;
    let ir = discover(root.path())?;
    let (mut sites, main) = selections(&ir)?;
    sites.push(PanicSiteSelection::new(sites[0].function(), 2));
    let decision = plan_closed_owned_panic_group(&ir, &sites, main).into_diagnostic()?;
    let ClosedPanicDecision::Ready(migration) = decision else {
        miette::bail!("reexport group refused: {decision:?}");
    };
    assert_eq!(migration.component().error_names().sources().len(), 2);
    apply_closed_panic_migration(&migration, false).into_diagnostic()?;
    build(root.path())?;
    let success = execute(root.path(), &[])?;
    assert!(success.status.success());
    assert_eq!(success.stdout, b"success\n");
    Ok(())
}

fn selections(ir: &dyn IrView) -> miette::Result<(Vec<PanicSiteSelection>, cordial::NodeId)> {
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let main = ir
        .node_by_path("main")
        .ok_or_else(|| miette::miette!("missing main"))?;
    Ok((
        vec![
            PanicSiteSelection::new(leaf, 0),
            PanicSiteSelection::new(leaf, 1),
        ],
        main,
    ))
}
