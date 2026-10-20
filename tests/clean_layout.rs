use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use cordial::{StoreLayout, SysrootCache};
use miette::IntoDiagnostic;

fn immediate_children(root: &std::path::Path) -> std::io::Result<BTreeSet<PathBuf>> {
    fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()
}

#[test]
fn project_store_classifies_every_created_directory() -> miette::Result<()> {
    cordial::init_tracing();
    let temp = tempfile::tempdir().into_diagnostic()?;
    let store = StoreLayout::from_root(temp.path().join("project"), "project");
    store.ensure_dirs().into_diagnostic()?;
    store.ensure_dirs().into_diagnostic()?;

    assert!(store.marker_path().is_file());
    let classified: BTreeSet<_> = store
        .regenerable_paths()
        .into_iter()
        .chain(store.curated_paths())
        .collect();
    for path in immediate_children(store.root()).into_diagnostic()? {
        if path.is_dir() {
            assert!(
                classified.contains(&path),
                "unclassified: {}",
                path.display()
            );
        }
    }
    assert!(classified.contains(&store.worklist_path()));
    assert!(!store.regenerable_paths().contains(&store.exceptions_dir()));
    Ok(())
}

#[test]
fn sysroot_classifies_its_created_cache_directory() -> miette::Result<()> {
    cordial::init_tracing();
    let temp = tempfile::tempdir().into_diagnostic()?;
    let sysroot = SysrootCache::from_home(temp.path());
    sysroot.ensure_dirs().into_diagnostic()?;
    sysroot.ensure_dirs().into_diagnostic()?;

    assert!(sysroot.marker_path().is_file());
    let classified: BTreeSet<_> = sysroot.regenerable_paths().into_iter().collect();
    for path in immediate_children(sysroot.root()).into_diagnostic()? {
        if path.is_dir() {
            assert!(
                classified.contains(&path),
                "unclassified: {}",
                path.display()
            );
        }
    }
    Ok(())
}
