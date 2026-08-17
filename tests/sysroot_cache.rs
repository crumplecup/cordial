#![cfg(feature = "homecoming_std")]

use cordial::{SysrootCache, default_store_home};
use miette::{IntoDiagnostic, WrapErr};

#[test]
fn sysroot_cache_defaults_under_cordial_home() {
    let home = default_store_home();
    let cache = SysrootCache::from_home(&home);
    assert_eq!(cache.root, home.join("sysroot"));
    assert_eq!(
        cache.rustdoc_cache_path("std"),
        home.join("sysroot/cache/rustdoc/std.json")
    );
}

#[test]
fn sysroot_cache_default_matches_from_home() {
    let expected = SysrootCache::from_home(default_store_home());
    assert_eq!(SysrootCache::default().root, expected.root);
}

#[test]
fn merged_std_inventory_excludes_unstable_simd_from_stable_scope() -> miette::Result<()> {
    use cordial::SysrootCache;
    use cordial::testing::{framework_std_type_items, load_merged_std_inventory};

    let sysroot = SysrootCache::default_cache();
    if !sysroot.rustdoc_cache_path("core").is_file() {
        return Ok(());
    }
    let core_text = std::fs::read_to_string(sysroot.rustdoc_cache_path("core"))
        .into_diagnostic()
        .wrap_err("read core")?;
    if !cordial::testing::rustdoc_json_has_stability_markers(&core_text) {
        return Ok(());
    }

    let merged = load_merged_std_inventory(&sysroot)
        .into_diagnostic()
        .wrap_err("merged inventory")?;
    let in_scope: Vec<_> = framework_std_type_items(&merged, false).collect();
    let simd: Vec<_> = in_scope
        .iter()
        .filter(|item| item.path.contains("core_simd"))
        .collect();
    assert!(
        simd.is_empty(),
        "stable scope should exclude core_simd: {:?}",
        simd.iter()
            .map(|item| &item.path)
            .take(5)
            .collect::<Vec<_>>()
    );
    Ok(())
}
