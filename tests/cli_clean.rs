use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use miette::IntoDiagnostic;

use cordial::{StoreLayout, SysrootCache, SysrootLock, WorklistLock, noop_progress};

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

#[test]
fn single_project_clean_waits_for_store_lock() -> miette::Result<()> {
    cordial::init_tracing();
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let slug = cordial::project_slug_from_path(project.path());
    let store = StoreLayout::from_root(home.path().join(&slug), slug);
    store.ensure_dirs().into_diagnostic()?;
    let lock = WorklistLock::exclusive(&store, "test gate", noop_progress()).into_diagnostic()?;
    let mut child = Command::new(env!("CARGO_BIN_EXE_cordial"))
        .arg("--project")
        .arg(project.path())
        .arg("--store-home")
        .arg(home.path())
        .arg("clean")
        .env("CORDIAL_NO_PROGRESS", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .into_diagnostic()?;
    std::thread::sleep(Duration::from_millis(300));
    assert!(child.try_wait().into_diagnostic()?.is_none());
    assert!(store.cache_dir().exists());
    drop(lock);
    let output = child.wait_with_output().into_diagnostic()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!store.cache_dir().exists());
    Ok(())
}

#[test]
fn all_skips_busy_store_and_sysroot_but_cleans_other_stores() -> miette::Result<()> {
    cordial::init_tracing();
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let busy = StoreLayout::from_root(home.path().join("busy"), "busy");
    let free = StoreLayout::from_root(home.path().join("free"), "free");
    busy.ensure_dirs().into_diagnostic()?;
    free.ensure_dirs().into_diagnostic()?;
    let sysroot = SysrootCache::from_home(home.path());
    sysroot.ensure_dirs().into_diagnostic()?;
    let _store_lock =
        WorklistLock::exclusive(&busy, "test gate", noop_progress()).into_diagnostic()?;
    let _sysroot_lock =
        SysrootLock::exclusive(&sysroot, "test build", noop_progress()).into_diagnostic()?;
    let output = run(project.path(), home.path(), &["--all"])?;
    assert!(!output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("Skipped"));
    assert!(text.contains("busy"));
    assert!(text.contains("sysroot"));
    assert!(busy.cache_dir().exists());
    assert!(sysroot.cache_dir().exists());
    assert!(!free.cache_dir().exists());
    Ok(())
}
