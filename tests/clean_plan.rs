use std::fs;

use miette::IntoDiagnostic;

use cordial::{CleanScope, StoreLayout, SysrootCache, plan_clean, run_clean};

#[test]
fn plans_exact_regenerable_paths_and_preserves_curated_data() -> miette::Result<()> {
    cordial::init_tracing();
    let home = tempfile::tempdir().into_diagnostic()?;
    let project = tempfile::tempdir().into_diagnostic()?;
    let store = StoreLayout::from_root(home.path().join("example"), "example");
    store.ensure_dirs().into_diagnostic()?;
    fs::create_dir_all(store.quality_patches_dir()).into_diagnostic()?;
    fs::create_dir_all(store.patches_dir()).into_diagnostic()?;
    fs::create_dir_all(store.config_dir()).into_diagnostic()?;
    fs::write(store.cache_dir().join("ir.json"), b"1234").into_diagnostic()?;
    fs::write(store.findings_dir().join("report.md"), b"abc").into_diagnostic()?;
    fs::write(store.worklist_path(), b"not a current worklist").into_diagnostic()?;
    fs::write(store.root().join("stray"), b"keep").into_diagnostic()?;

    let plan = plan_clean(CleanScope::Project {
        store: store.clone(),
        project_root: project.path().to_path_buf(),
    })
    .into_diagnostic()?;
    assert_eq!(plan.paths().len(), 3);
    assert!(plan.total_bytes() >= 7);
    assert!(store.cache_dir().exists());
    let report = run_clean(plan);
    assert!(report.complete(), "{:?}", report.failures());
    assert_eq!(report.removed().len(), 3);
    assert!(!store.cache_dir().exists());
    assert!(!store.findings_dir().exists());
    assert!(!store.worklist_path().exists());
    for path in store.curated_paths() {
        assert!(path.exists(), "{}", path.display());
    }
    assert!(store.root().join("stray").exists());
    assert!(store.marker_path().exists());
    Ok(())
}

#[test]
fn all_recognizes_legacy_stores_and_sysroot_but_skips_unknown_children() -> miette::Result<()> {
    cordial::init_tracing();
    let home = tempfile::tempdir().into_diagnostic()?;
    let project = tempfile::tempdir().into_diagnostic()?;
    let legacy = StoreLayout::from_root(home.path().join("legacy"), "legacy");
    fs::create_dir_all(legacy.cache_dir()).into_diagnostic()?;
    fs::create_dir_all(legacy.findings_dir()).into_diagnostic()?;
    fs::create_dir_all(legacy.exceptions_dir()).into_diagnostic()?;
    fs::create_dir_all(home.path().join("unknown/cache")).into_diagnostic()?;
    let sysroot = SysrootCache::from_home(home.path());
    fs::create_dir_all(sysroot.rustdoc_cache_dir()).into_diagnostic()?;
    fs::create_dir_all(sysroot.builds_dir()).into_diagnostic()?;
    let plan = plan_clean(CleanScope::All {
        store_home: home.path().to_path_buf(),
        project_root: project.path().to_path_buf(),
    })
    .into_diagnostic()?;
    assert!(
        plan.paths()
            .iter()
            .any(|item| item.path() == &legacy.cache_dir())
    );
    assert!(
        plan.paths()
            .iter()
            .any(|item| item.path() == &sysroot.cache_dir())
    );
    assert!(
        !plan
            .paths()
            .iter()
            .any(|item| item.path().starts_with(home.path().join("unknown")))
    );
    assert!(
        plan.skipped()
            .iter()
            .any(|item| item.path() == &home.path().join("unknown"))
    );
    Ok(())
}

#[test]
fn project_source_under_store_is_refused() -> miette::Result<()> {
    cordial::init_tracing();
    let home = tempfile::tempdir().into_diagnostic()?;
    let store = StoreLayout::from_root(home.path().join("project"), "project");
    store.ensure_dirs().into_diagnostic()?;
    let project = store.root().join("src");
    fs::create_dir_all(&project).into_diagnostic()?;
    let plan = plan_clean(CleanScope::Project {
        store: store.clone(),
        project_root: project,
    })
    .into_diagnostic()?;
    assert!(plan.paths().is_empty());
    assert_eq!(plan.skipped().len(), 1);
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlink_in_cache_does_not_remove_its_target() -> miette::Result<()> {
    cordial::init_tracing();
    let home = tempfile::tempdir().into_diagnostic()?;
    let project = tempfile::tempdir().into_diagnostic()?;
    let store = StoreLayout::from_root(home.path().join("example"), "example");
    store.ensure_dirs().into_diagnostic()?;
    let target = project.path().join("valuable");
    fs::write(&target, b"keep").into_diagnostic()?;
    std::os::unix::fs::symlink(&target, store.cache_dir().join("link")).into_diagnostic()?;
    let plan = plan_clean(CleanScope::Project {
        store,
        project_root: project.path().to_path_buf(),
    })
    .into_diagnostic()?;
    let report = run_clean(plan);
    assert!(report.complete());
    assert!(target.exists());
    Ok(())
}
