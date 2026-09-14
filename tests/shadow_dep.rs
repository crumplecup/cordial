#![cfg(all(feature = "rustdoc", feature = "shadow"))]

use std::path::Path;

use cordial::testing::collect_member_dep_build_config;
use cordial::{StoreLayout, resolve_shadow_dep_build_config};

#[test]
fn shadow_dep_cache_stem_matches_elicit_doc() {
    cordial::init_tracing();
    assert_eq!(
        StoreLayout::shadow_dep_cache_stem("elicit_url", "url"),
        "shadow-dep-elicit_url-url"
    );
}

#[test]
fn tracked_target_fallback_features_for_url_pair() {
    cordial::init_tracing();
    let config = resolve_shadow_dep_build_config(
        Path::new("tests/parity/workspaces/minimal-workspace"),
        "elicit_url",
        "url",
    );
    assert!(config.activated_features().contains(&"serde".to_string()));
}

#[test]
fn optional_dep_resolves_its_activating_member_feature() {
    cordial::init_tracing();
    let config = collect_member_dep_build_config(
        Path::new("tests/parity/workspaces/optional-dep-workspace"),
        "opt_member",
        "opt_upstream",
    )
    .expect("opt_member depends on opt_upstream");
    assert_eq!(
        config.activating_member_feature(),
        &Some("widget".to_string())
    );
}

#[test]
fn non_optional_dep_has_no_activating_member_feature() {
    cordial::init_tracing();
    let config = collect_member_dep_build_config(
        Path::new("tests/parity/workspaces/minimal-workspace"),
        "elicitation",
        "url",
    )
    .expect("elicitation depends on url");
    assert_eq!(config.activating_member_feature(), &None);
}
