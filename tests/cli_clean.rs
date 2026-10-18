use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use miette::IntoDiagnostic;

use cordial::{StoreLayout, SysrootCache};

fn run(project: &Path, home: &Path, args: &[&str]) -> miette::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(project)
        .arg("--store-home")
        .arg(home)
        .arg("clean")
        .args(args)
        .env("CORDIAL_NO_PROGRESS", "1")
        .output()
        .into_diagnostic()
}

#[test]
fn dry_run_and_clean_use_the_same_allowlist() -> miette::Result<()> {
    cordial::init_tracing();
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let slug = cordial::project_slug_from_path(project.path());
    let store = StoreLayout::from_root(home.path().join(&slug), slug);
    store.ensure_dirs().into_diagnostic()?;
    fs::write(store.cache_dir().join("artifact"), b"cache").into_diagnostic()?;
    fs::write(store.findings_dir().join("report"), b"report").into_diagnostic()?;
    fs::write(store.root().join("stray"), b"keep").into_diagnostic()?;
    let preview = run(project.path(), home.path(), &["--dry-run"])?;
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview_text = String::from_utf8_lossy(&preview.stdout);
    assert!(preview_text.contains("Would remove:"));
    assert!(preview_text.contains("Nothing was removed."));
    assert!(store.cache_dir().exists());
    let cleaned = run(project.path(), home.path(), &[])?;
    assert!(
        cleaned.status.success(),
        "{}",
        String::from_utf8_lossy(&cleaned.stderr)
    );
    assert!(!store.cache_dir().exists());
    assert!(!store.findings_dir().exists());
    assert!(store.exceptions_dir().exists());
    assert!(store.root().join("stray").exists());
    Ok(())
}

#[test]
fn all_cleans_recognized_stores_and_shared_sysroot_only() -> miette::Result<()> {
    cordial::init_tracing();
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let store = StoreLayout::from_root(home.path().join("other"), "other");
    store.ensure_dirs().into_diagnostic()?;
    fs::create_dir_all(home.path().join("unknown/cache")).into_diagnostic()?;
    let sysroot = SysrootCache::from_home(home.path());
    sysroot.ensure_dirs().into_diagnostic()?;
    fs::write(home.path().join("cordial.toml"), b"keep").into_diagnostic()?;
    let output = run(project.path(), home.path(), &["--all"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!store.cache_dir().exists());
    assert!(!sysroot.cache_dir().exists());
    assert!(home.path().join("unknown/cache").exists());
    assert!(home.path().join("cordial.toml").exists());
    Ok(())
}

#[test]
fn refuses_store_that_contains_project_source() -> miette::Result<()> {
    cordial::init_tracing();
    let home = tempfile::tempdir().into_diagnostic()?;
    let project = home.path().join("source");
    fs::create_dir_all(&project).into_diagnostic()?;
    let slug = cordial::project_slug_from_path(&project);
    let store = StoreLayout::from_root(home.path().join(&slug), slug);
    store.ensure_dirs().into_diagnostic()?;
    let output = run(&project, home.path(), &[])?;
    assert!(!output.status.success());
    assert!(store.cache_dir().exists());
    Ok(())
}
