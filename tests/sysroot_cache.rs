#![cfg(feature = "homecoming_std")]

use cordial::{SysrootCache, default_store_home};
#[cfg(feature = "slow_tests")]
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

// Expensive and environment-dependent: when a real local rustdoc-JSON
// sysroot cache for `core` exists, this parses and processes the whole
// thing (100+MB in practice) -- confirmed the hard way, over 60s pinning
// a core at ~100% CPU under a plain `cargo test --all-features` run,
// with zero opt-in signal. Gated behind `slow_tests` (see Cargo.toml's
// own comment on that feature) so routine verification doesn't pay this
// cost. Run explicitly with `just test-slow`.
#[cfg(feature = "slow_tests")]
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

#[test]
fn is_std_family_crate_recognizes_std_core_alloc() {
    use cordial::is_std_family_crate;

    assert!(is_std_family_crate("std"));
    assert!(is_std_family_crate("core"));
    assert!(is_std_family_crate("alloc"));
    assert!(!is_std_family_crate("homecoming_core"));
}

#[test]
fn resolve_sysroot_library_manifest_finds_std_when_nightly_installed() -> miette::Result<()> {
    use cordial::resolve_sysroot_library_manifest;

    let Ok(manifest) = resolve_sysroot_library_manifest("std") else {
        return Ok(());
    };
    assert!(manifest.ends_with("library/std/Cargo.toml"));
    assert!(manifest.is_file());
    Ok(())
}
