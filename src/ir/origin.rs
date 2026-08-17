//! Loader origin markers for dual-inventory crates.

/// Attribute key recording which loader materialized a node.
pub const ATTR_IR_ORIGIN: &str = "ir_origin";
/// Origin value for source inventory nodes.
pub const ORIGIN_SOURCE: &str = "source";
/// Origin value for rustdoc inventory nodes.
pub const ORIGIN_RUSTDOC: &str = "rustdoc";
/// Cross-reference to the matching item node from the other loader.
pub const ATTR_SYN_DOC_PEER: &str = "syn_doc_peer";
