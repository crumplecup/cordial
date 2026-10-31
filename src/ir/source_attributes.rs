//! Snapshot-bound attribute facts consumed without accessing a syntax backend.

use serde::{Deserialize, Serialize};

mod compatibility;
mod instrument;
mod native;
mod native_types;
pub use instrument::{SourceAttributeArgument, SourceAttributeArguments, SourceInstrumentSyntax};

pub use compatibility::{is_cfg_test, is_gated_instrument_attr, is_instrument_attr};

/// File attribute inventory on a source module node.
pub const ATTR_SOURCE_FILE_ATTRIBUTES: &str = "source_file_attributes";
/// Snapshot-bound syntax on a materialized attribute node.
pub const ATTR_SOURCE_ATTRIBUTE: &str = "source_attribute";

/// Crate-root lint declarations, including conditional declarations.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters,
)]
pub struct SourceCrateLints {
    #[getter(copy)]
    forbid_unsafe_code: bool,
    #[getter(copy)]
    missing_docs_at_least_warn: bool,
}

/// An attribute in the ordinary graph's attribute representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceAttribute {
    path: String,
    meta: String,
    /// Complete attribute range, including `#[..]`; doc comments have no range.
    range: Option<super::SourceByteRange>,
    #[getter(copy)]
    instrument: bool,
    /// Located argument evidence; absent when an instrument form is unsupported.
    instrument_syntax: Option<SourceInstrumentSyntax>,
}

/// Attributes belonging to one supported item, with a file-local qualified path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceAttributedItem {
    path: String,
    range: super::SourceByteRange,
    #[getter(copy)]
    line: u32,
    attributes: Vec<SourceAttribute>,
}

/// Attribute evidence collected during loading from the canonical file tree.
/// Test-only modules and unsupported owners are excluded from item attachment,
/// matching the attribute enricher's contract. This is not a census of all attrs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceFileAttributes {
    #[getter(copy)]
    source_digest: [u8; 32],
    /// Zero-based line before the first item, or end of an itemless file.
    #[getter(copy)]
    insert_at: usize,
    lints: SourceCrateLints,
    /// Lexical marker used together with the configured Verus target identity.
    #[getter(copy)]
    assume_specification: bool,
    items: Vec<SourceAttributedItem>,
}

/// Canonical spacing for syntax tokens, without parsing a rendered attribute.
#[tracing::instrument(level = "debug")]
fn render_tokens(parts: Vec<String>) -> String {
    let mut parts = parts.into_iter().peekable();
    let mut rendered = Vec::new();
    while let Some(part) = parts.next() {
        if part == ":" && parts.peek().is_some_and(|next| next == ":") {
            parts.next();
            rendered.push("::".to_string());
        } else if part == "'" {
            rendered.push(format!("'{0}", parts.next().unwrap_or_default()));
        } else {
            rendered.push(part);
        }
    }
    rendered.join(" ")
}
