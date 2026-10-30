#![cfg(feature = "panics")]

use cordial::{
    ClosedPanicDecision, ClosedPanicMigration, IrView, apply_closed_panic_migration,
    plan_closed_owned_panic_component,
};
use miette::IntoDiagnostic;
use std::fs;

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

fn migration(root: &std::path::Path) -> miette::Result<ClosedPanicMigration> {
    let ir = discover(root)?;
    assert!(function_facts(&ir, "main")?.entry_contract().is_some());
    let leaf = ir
        .node_by_path("leaf")
        .ok_or_else(|| miette::miette!("missing leaf"))?;
    let main = ir
        .node_by_path("main")
        .ok_or_else(|| miette::miette!("missing main"))?;
    let ClosedPanicDecision::Ready(plan) =
        plan_closed_owned_panic_component(&ir, leaf, 0, main).into_diagnostic()?
    else {
        miette::bail!("missing closed migration");
    };
    Ok(plan)
}

#[test]
fn dry_run_and_real_write_share_the_complete_closed_plan() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_main")?;
    let path = fixture.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let plan = migration(fixture.path())?;
    let preview = plan
        .component()
        .plan()
        .preview(&original)
        .into_diagnostic()?;
    let dry_run = apply_closed_panic_migration(&plan, true).into_diagnostic()?;
    assert_eq!(dry_run.planned_files(), 1);
    assert_eq!(dry_run.written_files(), 0);
    assert_eq!(dry_run.converted_sites(), 1);
    assert_eq!(dry_run.changed_functions(), 8);
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    assert_eq!(
        fs::read_dir(
            path.parent()
                .ok_or_else(|| miette::miette!("missing parent"))?
        )
        .into_diagnostic()?
        .count(),
        1
    );

    let applied = apply_closed_panic_migration(&plan, false).into_diagnostic()?;
    assert_eq!(applied.written_files(), 1);
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, preview);
    build(fixture.path())?;
    let success = execute(fixture.path(), &[])?;
    assert!(success.status.success());
    assert_eq!(success.stdout, b"success\n");
    let failure = execute(fixture.path(), &["fail"])?;
    assert_eq!(failure.status.code(), Some(1));
    assert!(
        String::from_utf8(failure.stderr)
            .into_diagnostic()?
            .contains("PermissionDenied")
    );
    for dry_run in [true, false] {
        assert!(apply_closed_panic_migration(&plan, dry_run).is_err());
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, preview);
    }
    Ok(())
}

#[test]
fn every_proof_input_and_source_membership_is_a_write_precondition() -> miette::Result<()> {
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_main")?;
    let path = fixture.path().join("src/main.rs");
    let untouched = fixture.path().join("src/support.rs");
    let manifest = fixture.path().join("Cargo.toml");
    let added = fixture.path().join("src/new_caller.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    let original_manifest = fs::read_to_string(&manifest).into_diagnostic()?;
    let support = "// a collected file outside the edited component\n";
    fs::write(&untouched, support).into_diagnostic()?;
    let plan = migration(fixture.path())?;
    assert_eq!(plan.inputs().len(), 3);
    for case in 0..5 {
        match case {
            0 => fs::write(&untouched, "// changed after closure\n").into_diagnostic()?,
            1 => fs::remove_file(&untouched).into_diagnostic()?,
            2 => fs::write(&added, "fn new_use() { let f = crate::leaf; let _ = f; }\n")
                .into_diagnostic()?,
            3 => fs::write(
                &manifest,
                format!("{original_manifest}\n# target policy changed\n"),
            )
            .into_diagnostic()?,
            _ => fs::write(&path, format!("{original}\n// changed\n")).into_diagnostic()?,
        }
        let before = fs::read(&path).into_diagnostic()?;
        for dry_run in [true, false] {
            assert!(
                apply_closed_panic_migration(&plan, dry_run).is_err(),
                "case {case}"
            );
            assert_eq!(fs::read(&path).into_diagnostic()?, before, "case {case}");
        }
        fs::write(&path, &original).into_diagnostic()?;
        fs::write(&untouched, support).into_diagnostic()?;
        fs::write(&manifest, &original_manifest).into_diagnostic()?;
        if added.exists() {
            fs::remove_file(&added).into_diagnostic()?;
        }
    }
    assert!(apply_closed_panic_migration(&plan, true).is_ok());
    Ok(())
}

#[test]
#[cfg(unix)]
fn write_preserves_permissions_and_rejects_source_symlinks() -> miette::Result<()> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    cordial::init_tracing();
    let fixture = copy_specimen("apply_error_main")?;
    let path = fixture.path().join("src/main.rs");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).into_diagnostic()?;
    let plan = migration(fixture.path())?;
    let outside = fixture.path().join("outside.rs");
    let original = fs::read(&path).into_diagnostic()?;
    fs::rename(&path, &outside).into_diagnostic()?;
    symlink(&outside, &path).into_diagnostic()?;
    for dry_run in [true, false] {
        assert!(apply_closed_panic_migration(&plan, dry_run).is_err());
        assert_eq!(fs::read(&outside).into_diagnostic()?, original);
        assert!(
            fs::symlink_metadata(&path)
                .into_diagnostic()?
                .file_type()
                .is_symlink()
        );
    }
    fs::remove_file(&path).into_diagnostic()?;
    fs::rename(&outside, &path).into_diagnostic()?;
    apply_closed_panic_migration(&plan, false).into_diagnostic()?;
    assert_eq!(
        fs::metadata(&path).into_diagnostic()?.permissions().mode() & 0o777,
        0o640
    );
    Ok(())
}
