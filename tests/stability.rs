#![cfg(feature = "rustdoc")]

use cordial::testing::{StabilityLevel, parse_stability_attr_text, stability_from_attrs};

const UNSTABLE_SIMD: &str = r#"#[attr = Stability {stability: Stability {level: Unstable {reason: None,
issue: 86656}, feature: "portable_simd"}}]"#;

const STABLE_DEBUG: &str = r#"#[attr = Stability {stability: Stability {level: Stable {since: Version(RustcVersion { major: 1, minor: 0, patch: 0 })},
feature: "rust1"}}]"#;

const ALLOW_ONLY: &str = "#[allow(non_camel_case_types)]";

#[test]
fn parse_stability_unstable_block() {
    cordial::init_tracing();
    assert_eq!(
        parse_stability_attr_text(UNSTABLE_SIMD),
        StabilityLevel::Unstable
    );
}

#[test]
fn parse_stability_stable_block() {
    cordial::init_tracing();
    assert_eq!(
        parse_stability_attr_text(STABLE_DEBUG),
        StabilityLevel::Stable
    );
}

#[test]
fn parse_stability_unknown_without_marker() {
    cordial::init_tracing();
    assert_eq!(
        parse_stability_attr_text(ALLOW_ONLY),
        StabilityLevel::Unknown
    );
}

#[test]
fn parse_stability_legacy_unstable_substring() {
    cordial::init_tracing();
    assert_eq!(
        parse_stability_attr_text("level: Unstable"),
        StabilityLevel::Unstable
    );
}

#[test]
fn stability_from_attrs_prefers_unstable_over_stable() {
    cordial::init_tracing();
    let attrs = vec![
        rustdoc_types::Attribute::Other(STABLE_DEBUG.to_string()),
        rustdoc_types::Attribute::Other(UNSTABLE_SIMD.to_string()),
    ];
    assert_eq!(stability_from_attrs(&attrs), StabilityLevel::Unstable);
}

#[test]
fn stability_from_attrs_non_other_attributes_are_ignored() {
    cordial::init_tracing();
    let attrs = vec![rustdoc_types::Attribute::NonExhaustive];
    assert_eq!(stability_from_attrs(&attrs), StabilityLevel::Unknown);
}
