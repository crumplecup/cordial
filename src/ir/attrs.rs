//! Canonical IR attribute keys (string keys + JSON values until typed AttrStore).
#![allow(dead_code)]

// Crate root (loader)
pub const ATTR_CRATE_VERSION: &str = "crate_version";

// Item identity (loader + structure enricher)
pub const ATTR_QUALIFIED_PATH: &str = "qualified_path";
pub const ATTR_RUSTDOC_KIND: &str = "rustdoc_kind";
pub const ATTR_ITEM_NAME: &str = "item_name";
pub const ATTR_IS_PUBLIC: &str = "is_public";

// Rustdoc structure (RustdocStructureEnricher)
pub const ATTR_IS_GENERIC: &str = "is_generic";
pub const ATTR_IS_UNSTABLE: &str = "is_unstable";
pub const ATTR_ALIAS_TARGET: &str = "alias_target";
pub const ATTR_PUBLIC_METHODS: &str = "public_methods";
pub const ATTR_TRAIT_IMPLS: &str = "trait_impls";
pub const ATTR_TRAIT_PREREQS: &str = "trait_prereqs";
pub const ATTR_ELICIT_COMPLETE: &str = "elicit_complete";
pub const ATTR_ELICIT_COMPLETE_FACTORY: &str = "elicit_complete_factory";
pub const ATTR_WRAPS_FOREIGN: &str = "wraps_foreign";

// Plugin enrichers (documented here; defined on enricher types)
pub const ATTR_SHADOW_PATH: &str = "shadow_path";
pub const ATTR_WRAPPER_COVERAGE: &str = "wrapper_coverage";
pub const ATTR_FEATURE_PROBE_CRATE: &str = "feature_probe_crate";
pub const ATTR_FEATURE_PROBE_CANDIDATE_FEATURES: &str = "feature_probe_candidate_unlock_features";
pub const ATTR_FEATURE_PROBE_PROBED_PREREQS: &str = "feature_probe_probed_prereqs";
pub const ATTR_PROOF_TEST: &str = "proof_test";
pub const ATTR_COMPOSITION_TEST: &str = "composition_test";
pub const ATTR_TRAIT_SHORT: &str = "trait_short";
