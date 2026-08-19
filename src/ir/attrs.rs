//! Canonical IR attribute keys (string keys + JSON values until typed AttrStore).

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
