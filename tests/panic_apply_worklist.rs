//! Exercise the production command, closed writes, and exact worklist claims.

use cordial::{ItemState, StoreLayout, Worklist};
use miette::IntoDiagnostic;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

#[path = "support/strategy_specimens.rs"]
mod specimens;
use specimens::{build, copy_specimen, discover, execute, function_facts};

const APPLY: &[&str] = &["quality", "--apply", "--etiquette", "panics"];

#[test]
fn command_previews_writes_and_claims_only_the_repaired_panic() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_main")?;
    let path = root.path().join("src/main.rs");
    let original = fs::read_to_string(&path).into_diagnostic()?;
    // An unsupported, successful expect remains a distinct manual item.
    let original = original.replace("fn main() {", "fn main() {\n    let _ = ::std::result::Result::<u32, ::std::io::Error>::Ok(1).expect(\"manual\");");
    fs::write(&path, &original).into_diagnostic()?;
    let ir = discover(root.path())?;
    assert!(function_facts(&ir, "leaf")?.unwraps()[0].line().is_some());
    let home = tempfile::tempdir().into_diagnostic()?;
    let store = store(root.path(), home.path());
    run(
        root.path(),
        home.path(),
        &["quality", "--etiquette", "panics"],
    )?;
    let before = list(&store)?;
    assert_eq!(before.items().len(), 2);
    let worklist_path = store.root().join("worklist.json");
    let bytes = fs::read(&worklist_path).into_diagnostic()?;
    let mut args = APPLY.to_vec();
    args.push("--dry-run");
    let dry = run(root.path(), home.path(), &args)?;
    let output = String::from_utf8_lossy(&dry.stdout);
    assert!(output.contains("1 site(s) would be converted"), "{output}");
    assert!(output.contains("1 candidate site(s) blocked"), "{output}");
    assert!(output.contains("CordialLeafSite0Error"), "{output}");
    assert!(output.contains("recursive_other"), "{output}");
    assert!(
        output.contains("Failure policy: panic becomes returned error"),
        "{output}"
    );
    assert!(
        output.contains("1 worklist item(s) would be checked off"),
        "{output}"
    );
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, original);
    assert_eq!(fs::read(&worklist_path).into_diagnostic()?, bytes);

    let applied = run(root.path(), home.path(), APPLY)?;
    let output = String::from_utf8_lossy(&applied.stdout);
    assert!(
        output.contains("1 worklist item(s) checked off"),
        "{output}"
    );
    assert!(
        output.contains("1 panic item(s) remain open for manual repair"),
        "{output}"
    );
    let after = list(&store)?;
    for item in after.items() {
        assert_eq!(
            *item.state(),
            if item.rule_id().as_deref() == Some("PANIC-SOURCE-UNWRAP") {
                ItemState::Done
            } else {
                ItemState::Open
            }
        );
    }
    build(root.path())?;
    let success = execute(root.path(), &[])?;
    assert!(success.status.success());
    assert_eq!(success.stdout, b"success\n");
    let failure = execute(root.path(), &["fail"])?;
    assert_eq!(failure.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failure.stderr).contains("PermissionDenied"));
    let repeated = run(root.path(), home.path(), APPLY)?;
    assert!(String::from_utf8_lossy(&repeated.stdout).contains("0 site(s) converted"));
    assert_eq!(list(&store)?, after);
    Ok(())
}

#[test]
fn policy_and_closure_blockers_preserve_source_and_claims() -> miette::Result<()> {
    cordial::init_tracing();
    for (policy, mutation, reason) in [
        (
            "[internal_error_chain]\nfield_strategy = \"borrowed\"\n",
            "",
            "BorrowedPolicy",
        ),
        ("", "fn hold() { let _ = leaf; }\n", "UnresolvedUse"),
        ("[panics]\nenabled = false\n", "", "0 site(s) converted"),
    ] {
        let root = copy_specimen("apply_error_main")?;
        let home = tempfile::tempdir().into_diagnostic()?;
        fs::write(root.path().join("cordial.toml"), policy).into_diagnostic()?;
        let path = root.path().join("src/main.rs");
        let source = format!("{}{mutation}", fs::read_to_string(&path).into_diagnostic()?);
        fs::write(&path, &source).into_diagnostic()?;
        let output = run(root.path(), home.path(), APPLY)?;
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(reason),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(fs::read_to_string(&path).into_diagnostic()?, source);
        assert!(
            Worklist::read(&store(root.path(), home.path()))
                .into_diagnostic()?
                .is_none()
        );
    }
    Ok(())
}

#[test]
fn changed_policy_prevents_claiming_an_old_worklist() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_main")?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let store = store(root.path(), home.path());
    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"borrowed\"\n",
    )
    .into_diagnostic()?;
    run(
        root.path(),
        home.path(),
        &["quality", "--etiquette", "panics"],
    )?;
    let before = list(&store)?;
    fs::write(
        root.path().join("cordial.toml"),
        "[internal_error_chain]\nfield_strategy = \"owned\"\n",
    )
    .into_diagnostic()?;
    let applied = run(root.path(), home.path(), APPLY)?;
    let output = String::from_utf8_lossy(&applied.stdout);
    assert!(output.contains("1 site(s) converted"), "{output}");
    assert!(output.contains("worklist is out of date"), "{output}");
    assert_eq!(list(&store)?, before);
    Ok(())
}

#[test]
fn central_exceptions_prevent_panic_migration() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_main")?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let store = store(root.path(), home.path());
    let exceptions = store.exceptions_dir().join("panics");
    fs::create_dir_all(&exceptions).into_diagnostic()?;
    fs::write(exceptions.join("strategy-specimen.json"), r#"[{"file":"src/main.rs","rule_id":"PANIC-SOURCE-UNWRAP","context":"leaf","reason":"fixture approval"}]"#).into_diagnostic()?;
    let path = root.path().join("src/main.rs");
    let original = fs::read(&path).into_diagnostic()?;
    let applied = run(root.path(), home.path(), APPLY)?;
    assert!(String::from_utf8_lossy(&applied.stdout).contains("0 site(s) converted"));
    assert_eq!(fs::read(&path).into_diagnostic()?, original);
    assert!(Worklist::read(&store).into_diagnostic()?.is_none());
    Ok(())
}

#[test]
fn multiple_sites_share_one_write_and_claim_every_repair() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_multiple")?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let store = store(root.path(), home.path());
    let path = root.path().join("src/main.rs");
    let original = fs::read(&path).into_diagnostic()?;
    run(
        root.path(),
        home.path(),
        &["quality", "--etiquette", "panics"],
    )?;
    assert_eq!(list(&store)?.items().len(), 3);
    let worklist_path = store.root().join("worklist.json");
    let before = fs::read(&worklist_path).into_diagnostic()?;
    let mut dry_args = APPLY.to_vec();
    dry_args.push("--dry-run");
    let dry = run(root.path(), home.path(), &dry_args)?;
    let output = String::from_utf8_lossy(&dry.stdout);
    assert!(output.contains("3 site(s) would be converted"), "{output}");
    assert!(
        output.contains("3 worklist item(s) would be checked off"),
        "{output}"
    );
    assert_eq!(fs::read(&path).into_diagnostic()?, original);
    assert_eq!(fs::read(&worklist_path).into_diagnostic()?, before);
    let applied = run(root.path(), home.path(), APPLY)?;
    let output = String::from_utf8_lossy(&applied.stdout);
    assert!(output.contains("3 site(s) converted"), "{output}");
    assert!(
        output.contains("3 worklist item(s) checked off"),
        "{output}"
    );
    assert!(
        list(&store)?
            .items()
            .iter()
            .all(|item| *item.state() == ItemState::Done)
    );
    build(root.path())?;
    assert_eq!(
        execute(root.path(), &[])?.stdout,
        b"1\n2\n3\n1\n2\nsuccess\n"
    );
    assert_eq!(execute(root.path(), &["third"])?.status.code(), Some(1));
    Ok(())
}

#[test]
fn one_unresolved_source_blocks_all_joint_claims_and_writes() -> miette::Result<()> {
    cordial::init_tracing();
    let root = copy_specimen("apply_error_multiple")?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let store = store(root.path(), home.path());
    let path = root.path().join("src/main.rs");
    let source = fs::read_to_string(&path)
        .into_diagnostic()?
        .replace("operation(3, fail).unwrap()", "Some(fail).unwrap()");
    fs::write(&path, &source).into_diagnostic()?;
    run(
        root.path(),
        home.path(),
        &["quality", "--etiquette", "panics"],
    )?;
    let before = list(&store)?;
    let output = run(root.path(), home.path(), APPLY)?;
    let output = String::from_utf8_lossy(&output.stdout);
    assert!(output.contains("0 site(s) converted"), "{output}");
    assert!(output.contains("3 candidate site(s) blocked"), "{output}");
    assert_eq!(fs::read_to_string(&path).into_diagnostic()?, source);
    assert_eq!(list(&store)?, before);
    Ok(())
}

fn store(root: &Path, home: &Path) -> StoreLayout {
    let slug = cordial::project_slug_from_path(root);
    StoreLayout::from_root(home.join(&slug), slug)
}

fn list(store: &StoreLayout) -> miette::Result<Worklist> {
    Worklist::read(store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("missing worklist"))
}

fn run(root: &Path, home: &Path, args: &[&str]) -> miette::Result<Output> {
    let result = Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(root)
        .arg("--store-home")
        .arg(home)
        .args(args)
        .env("CORDIAL_NO_PROGRESS", "1")
        .output()
        .into_diagnostic()?;
    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(result)
}
