//! Built-in assignments. Dynamic amenable-ext targets share one measured rule.

use tracing::instrument;

/// Built-in etiquette overrides to the default `quick` tier.
///
/// The measured `amenable-ext-*` family is handled by [`builtin_tier`] because
/// projects can configure target names that are not known at compile time.
pub const BUILTIN_TIER_ASSIGNMENTS: &[(&str, &str)] = &[
    ("amenable-std", "deep"),
    ("antipatterns", "build"),
    ("creusot_diagnostics", "deep"),
    ("dependency_freshness", "deep"),
    ("doc_warnings", "build"),
    ("feature_warnings", "deep"),
    ("homecoming-std", "build"),
    ("impl-coverage", "deep"),
    ("verus_warnings", "deep"),
];

/// Look up a built-in tier; `None` lets the etiquette supply its own default.
#[instrument(level = "trace")]
pub fn builtin_tier(id: &str) -> Option<&'static str> {
    if id.starts_with("amenable-ext-") {
        return Some("build");
    }
    BUILTIN_TIER_ASSIGNMENTS
        .iter()
        .find_map(|(candidate, tier)| (*candidate == id).then_some(*tier))
}
