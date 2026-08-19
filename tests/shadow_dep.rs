#![cfg(all(feature = "rustdoc", feature = "shadow"))]

use std::path::Path;

use cordial::{StoreLayout, resolve_shadow_dep_build_config};

#[test]
fn shadow_dep_cache_stem_matches_elicit_doc() {
    assert_eq!(
        StoreLayout::shadow_dep_cache_stem("elicit_url", "url"),
        "shadow-dep-elicit_url-url"
    );
}

#[test]
fn tracked_target_fallback_features_for_url_pair() {
    let config = resolve_shadow_dep_build_config(
        Path::new("tests/parity/workspaces/minimal-workspace"),
        "elicit_url",
        "url",
    );
    assert!(config.activated_features.contains(&"serde".to_string()));
}
