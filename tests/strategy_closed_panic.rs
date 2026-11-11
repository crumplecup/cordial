#![cfg(feature = "panics")]

use cordial::{
    ATTR_BINARY_ENTRY_PATHS, ATTR_CARGO_MANIFEST_SNAPSHOT, ATTR_CRATE_KINDS,
    ATTR_FUNCTION_SOURCE_SYNTAX, ATTR_SOURCE_MODULE_SCOPE, ClosedPanicBlocker as Blocker,
    ClosedPanicDecision as Decision, EdgeKind, ErrorDeclarationDecision, IrView,
    PanicComponentBlocker, ReturnLiftBlocker, ReturnLiftDecision,
    plan_closed_owned_panic_component, plan_owned_error_declaration, plan_owned_main_return_lift,
};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

#[test]
fn a_closed_binary_migration_preserves_success_and_returns_typed_failure() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_main")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    build(fixture.path())?;
    assert_eq!(execute(fixture.path(), &[])?.stdout, b"success\n");
    assert!(!execute(fixture.path(), &["fail"])?.status.success());
    let ir = discover(fixture.path())?;
    assert_eq!(
        ir.binary_entry_paths().into_diagnostic()?,
        Some(vec![path.clone()])
    );
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let main = ir
        .node_by_path("main")
        .ok_or_else(|| miette::miette!("missing main"))?;
    let Decision::Ready(migration) =
        plan_closed_owned_panic_component(&ir, leaf, 0, main).into_diagnostic()?
    else {
        miette::bail!("migration was not closed");
    };
    assert_eq!(migration.component().functions().len(), 8);
    let rendered = migration
        .component()
        .plan()
        .preview(&original)
        .into_diagnostic()?;
    assert!(rendered.contains("fn main() -> ::core::result::Result<(), CordialLeafSite0Error>"));
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    fs::write(&path, &rendered).into_diagnostic()?;
    build(fixture.path())?;
    let success = execute(fixture.path(), &[])?;
    assert!(success.status.success());
    assert_eq!(success.stdout, b"success\n");
    assert!(success.stderr.is_empty());
    let failure = execute(fixture.path(), &["fail"])?;
    assert_eq!(failure.status.code(), Some(1));
    assert!(
        failure.stdout.is_empty(),
        "failure must not continue into success reporting"
    );
    let error = String::from_utf8(failure.stderr).into_diagnostic()?;
    assert!(error.starts_with("Error: CordialLeafSite0Error"), "{error}");
    assert!(error.contains("PermissionDenied"));
    assert!(error.contains("file: \"src/main.rs\""), "{error}");
    let site_line = original
        .lines()
        .position(|line| line.contains("    operation(fail)"))
        .ok_or_else(|| miette::miette!("missing panic site"))?
        + 1;
    assert!(error.contains(&format!("line: {site_line}")), "{error}");
    assert!(!error.contains("panicked at"));
    let fresh = discover(fixture.path())?;
    assert_eq!(
        plan_closed_owned_panic_component(
            &fresh,
            fresh
                .node_by_path("leaf")
                .ok_or_else(|| miette::miette!("missing leaf"))?,
            0,
            fresh
                .node_by_path("main")
                .ok_or_else(|| miette::miette!("missing main"))?
        )
        .into_diagnostic()?,
        Decision::NoCandidate
    );
    Ok(())
}

#[test]
fn main_adapter_is_independent_and_requires_cargo_and_signature_evidence() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_main")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let mut ir = discover(fixture.path())?;
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let main = ir
        .node_by_path("main")
        .ok_or_else(|| miette::miette!("missing main"))?;
    let ErrorDeclarationDecision::Ready(declaration) =
        plan_owned_error_declaration(&ir, leaf, 0).into_diagnostic()?
    else {
        miette::bail!("missing declaration");
    };
    let ReturnLiftDecision::Ready(adapter) =
        plan_owned_main_return_lift(&ir, main, &declaration).into_diagnostic()?
    else {
        miette::bail!("missing main adapter");
    };
    let adapted = adapter.preview(&original).into_diagnostic()?;
    let rendered = format!(
        "{adapted}{}",
        declaration
            .plan()
            .edits()
            .iter()
            .map(|edit| edit.replacement().as_str())
            .collect::<String>()
    );
    fs::write(&path, rendered).into_diagnostic()?;
    build(fixture.path())?;
    assert_eq!(execute(fixture.path(), &[])?.stdout, b"success\n");
    assert!(
        !execute(fixture.path(), &["fail"])?.status.success(),
        "isolated success adapter must leave the leaf panic unchanged"
    );
    assert!(function_facts(&ir, "main")?.entry_contract().is_some());
    let root = ir.root();
    for (key, value) in [
        (ATTR_BINARY_ENTRY_PATHS, serde_json::json!([])),
        (ATTR_BINARY_ENTRY_PATHS, serde_json::json!([path, path])),
        (
            ATTR_BINARY_ENTRY_PATHS,
            serde_json::json!(["/another/main.rs"]),
        ),
        (ATTR_CRATE_KINDS, serde_json::json!(["lib", "bin"])),
    ] {
        let original_attr = ir
            .node(root)
            .and_then(|node| node.attr(key).cloned())
            .ok_or_else(|| miette::miette!("missing {key}"))?;
        ir.set_attr(root, key, value).into_diagnostic()?;
        assert_eq!(
            plan_owned_main_return_lift(&ir, main, &declaration).into_diagnostic()?,
            ReturnLiftDecision::Refused(ReturnLiftBlocker::UnsupportedEntryPoint)
        );
        ir.set_attr(root, key, original_attr).into_diagnostic()?;
    }
    let mut facts = serde_json::to_value(function_facts(&ir, "main")?).into_diagnostic()?;
    facts
        .as_object_mut()
        .ok_or_else(|| miette::miette!("missing facts"))?
        .remove("entry_contract");
    ir.set_attr(main, ATTR_FUNCTION_SOURCE_SYNTAX, facts)
        .into_diagnostic()?;
    assert_eq!(
        plan_owned_main_return_lift(&ir, main, &declaration).into_diagnostic()?,
        ReturnLiftDecision::Refused(ReturnLiftBlocker::UnsupportedEntryPoint)
    );
    Ok(())
}

#[test]
fn undiscovered_callers_function_values_and_opaque_uses_prevent_closure() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_main")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    for extra in [
        "fn callback() { let f = leaf; let _ = f; }",
        "fn callback() { let _ = crate::leaf(false, &mut 0); }",
        "fn callback() { let f = || leaf(false, &mut 0); let _ = f; }",
        "const POINTER: fn(bool, &mut u32) -> u32 = leaf;",
        "struct Other; impl Other { fn call() { let _ = leaf(false, &mut 0); } }",
        "fn callback() { println!(\"{:?}\", leaf(false, &mut 0)); }",
    ] {
        fs::write(&path, format!("{original}\n{extra}\n")).into_diagnostic()?;
        let ir = discover(fixture.path())?;
        let leaf = ir
            .node_by_path("leaf")
            .ok_or_else(|| miette::miette!("missing leaf"))?;
        let main = ir
            .node_by_path("main")
            .ok_or_else(|| miette::miette!("missing main"))?;
        let decision = plan_closed_owned_panic_component(&ir, leaf, 0, main).into_diagnostic()?;
        assert!(
            // Spellings the use inventory cannot follow are refused as unresolved or
            // opaque. A call the database resolves anyway reaches the leaf as a
            // known caller, and that caller is open: either way closure is refused.
            matches!(
                decision,
                Decision::Refused(
                    Blocker::UnresolvedUse { .. }
                        | Blocker::OpaqueSyntax { .. }
                        | Blocker::Component(PanicComponentBlocker::OpenCaller(_))
                )
            ),
            "{extra}: {decision:?}"
        );
        assert_eq!(
            fs::read_to_string(&path).into_diagnostic()?,
            format!("{original}\n{extra}\n")
        );
    }
    Ok(())
}

#[test]
fn missing_and_inconsistent_inventory_never_establish_closure() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_main")?;
    let mut ir = discover(fixture.path())?;
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let main = ir
        .node_by_path("main")
        .ok_or_else(|| miette::miette!("missing main"))?;
    let root = ir.root();
    let manifest = ir
        .node(root)
        .and_then(|node| node.attr(ATTR_CARGO_MANIFEST_SNAPSHOT).cloned())
        .ok_or_else(|| miette::miette!("missing manifest snapshot"))?;
    let mut wrong_manifest = manifest.clone();
    wrong_manifest["path"] = serde_json::json!("/another/Cargo.toml");
    ir.set_attr(root, ATTR_CARGO_MANIFEST_SNAPSHOT, wrong_manifest)
        .into_diagnostic()?;
    assert_eq!(
        plan_closed_owned_panic_component(&ir, leaf, 0, main).into_diagnostic()?,
        Decision::Refused(Blocker::MissingManifestSnapshot)
    );
    ir.set_attr(root, ATTR_CARGO_MANIFEST_SNAPSHOT, serde_json::Value::Null)
        .into_diagnostic()?;
    assert!(plan_closed_owned_panic_component(&ir, leaf, 0, main).is_err());
    ir.set_attr(root, ATTR_CARGO_MANIFEST_SNAPSHOT, manifest)
        .into_diagnostic()?;
    let module = ir.parents(leaf, EdgeKind::Contains)[0];
    let original = serde_json::to_value(ir.source_module_scope(module).into_diagnostic()?)
        .into_diagnostic()?;
    let mut missing = original.clone();
    missing
        .as_object_mut()
        .ok_or_else(|| miette::miette!("missing scope"))?
        .remove("uses");
    ir.set_attr(module, ATTR_SOURCE_MODULE_SCOPE, missing)
        .into_diagnostic()?;
    assert_eq!(
        plan_closed_owned_panic_component(&ir, leaf, 0, main).into_diagnostic()?,
        Decision::Refused(Blocker::MissingUseInventory(module))
    );
    let mut incomplete = original;
    incomplete["uses"]["paths"] = serde_json::json!([]);
    ir.set_attr(module, ATTR_SOURCE_MODULE_SCOPE, incomplete)
        .into_diagnostic()?;
    assert!(matches!(
        plan_closed_owned_panic_component(&ir, leaf, 0, main).into_diagnostic()?,
        Decision::Refused(Blocker::IncompleteUseEvidence(_))
    ));
    Ok(())
}

#[test]
fn cargo_entry_identity_and_other_source_files_constrain_closure() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_main")?;
    let path = fixture.path().join("src/main.rs");
    let manifest = fixture.path().join("Cargo.toml");
    let original_manifest = fs::read_to_string(&manifest).into_diagnostic()?;
    fs::copy(&path, fixture.path().join("src/actual.rs")).into_diagnostic()?;
    fs::write(
        &manifest,
        original_manifest
            .replace("edition = \"2024\"", "edition = \"2024\"\nautobins = false")
            .replace("src/main.rs", "src/actual.rs"),
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    assert_eq!(
        ir.binary_entry_paths().into_diagnostic()?,
        Some(vec![fixture.path().join("src/actual.rs")])
    );
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let main = ir
        .node_by_path("main")
        .ok_or_else(|| miette::miette!("missing main"))?;
    assert_eq!(
        plan_closed_owned_panic_component(&ir, leaf, 0, main).into_diagnostic()?,
        Decision::Refused(Blocker::UnsupportedEntryPoint)
    );

    fs::write(&manifest, original_manifest).into_diagnostic()?;
    fs::write(
        fixture.path().join("src/actual.rs"),
        "fn callback() { let _ = crate::leaf(false, &mut 0); }",
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let main = ir
        .node_by_path("main")
        .ok_or_else(|| miette::miette!("missing main"))?;
    assert!(matches!(
        plan_closed_owned_panic_component(&ir, leaf, 0, main).into_diagnostic()?,
        Decision::Refused(Blocker::UnresolvedUse { .. })
    ));
    Ok(())
}
