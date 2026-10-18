use std::time::Duration;

use cordial::{builtin_tier, tier_for_timing};

#[test]
fn measured_costs_partition_at_visible_gaps() {
    cordial::init_tracing();
    let cases = [
        ("panics", 3.84, 3.74, "quick"),
        ("tracing", 6.19, 6.01, "quick"),
        ("amenable-ext-jiff", 39.78, 2.36, "build"),
        ("antipatterns", 252.74, 4.79, "build"),
        ("doc_warnings", 146.15, 3.05, "build"),
        ("dependency_freshness", 9.54, 9.53, "deep"),
        ("impl-coverage", 157.75, 10.73, "deep"),
        ("verus_warnings", 33.92, 34.10, "deep"),
        ("amenable-std", 39.55, 38.04, "deep"),
    ];
    for (id, cold, warm, expected) in cases {
        let tier = tier_for_timing(Duration::from_secs_f64(cold), Duration::from_secs_f64(warm));
        assert_eq!(tier, expected, "{id}");
    }
}

#[test]
fn configured_amenable_ext_targets_share_the_build_assignment() {
    cordial::init_tracing();
    assert_eq!(builtin_tier("amenable-ext-chrono"), Some("build"));
    assert_eq!(builtin_tier("amenable-ext-chrono-tz"), Some("build"));
    assert_eq!(builtin_tier("custom-plugin"), None);
}
