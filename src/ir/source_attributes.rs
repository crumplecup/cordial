//! Snapshot-bound attribute facts consumed without accessing a syntax backend.

use serde::{Deserialize, Serialize};

mod instrument;
mod native;
#[cfg(feature = "tracing")]
pub(crate) use native::AttributeIdentity;
pub(crate) use native::AttributePathIdentity;
mod native_types;
pub use instrument::{SourceAttributeArgument, SourceAttributeArguments, SourceInstrumentSyntax};
#[cfg(feature = "tracing")]
pub(crate) use native_types::type_key as native_type_key;

/// File attribute inventory on a source module node.
pub const ATTR_SOURCE_FILE_ATTRIBUTES: &str = "source_file_attributes";
/// Snapshot-bound syntax on a materialized attribute node.
pub const ATTR_SOURCE_ATTRIBUTE: &str = "source_attribute";
/// The attributes of an item as a cfg context read them, on the item's node, for the items whose attributes the ordinary
/// build could not resolve.
pub const ATTR_SOURCE_CONTEXT_ATTRIBUTES: &str = "source_context_attributes";

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
    /// Whether `instrument` could not be decided: the attribute is in code that has no scope in the cfg the file was read
    /// under (a module of no crate's tree, or an item whose `cfg` guard is off), so what its path names was not
    /// resolved. `instrument` is `false` then, and it is not an answer.
    #[serde(default)]
    #[getter(copy)]
    instrument_unresolved: bool,
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

impl SourceAttributedItem {
    /// This item with each attribute that was not resolved taken from the first of `others` that resolved it.
    /// The others are the same item read in a cfg context, so an attribute is the one at the same place.
    #[must_use]
    pub(crate) fn resolved_by<'a>(&self, others: impl Iterator<Item = &'a SourceAttributedItem> + Clone) -> Self {
        let attributes = self
            .attributes
            .iter()
            .enumerate()
            .map(|(index, attribute)| {
                if !attribute.instrument_unresolved {
                    return attribute.clone();
                }
                others
                    .clone()
                    .filter_map(|other| other.attributes.get(index))
                    .find(|other| !other.instrument_unresolved && other.meta == attribute.meta)
                    .unwrap_or(attribute)
                    .clone()
            })
            .collect();
        Self { attributes, ..self.clone() }
    }

    /// Whether any attribute of the item was not resolved.
    pub(crate) fn has_unresolved(&self) -> bool {
        self.attributes.iter().any(|attribute| attribute.instrument_unresolved)
    }
}

/// An item's attributes as a cfg context read them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceContextAttributes {
    context: crate::ir::SemanticContextKey,
    item: SourceAttributedItem,
}

impl SourceContextAttributes {
    pub(crate) fn new(context: crate::ir::SemanticContextKey, item: SourceAttributedItem) -> Self {
        Self { context, item }
    }
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

impl SourceFileAttributes {
    /// Whether any attribute of any item was not resolved.
    pub(crate) fn has_unresolved(&self) -> bool {
        self.items.iter().any(SourceAttributedItem::has_unresolved)
    }

    /// These attributes with `items` in place of the items.
    #[must_use]
    pub(crate) fn with_items(&self, items: Vec<SourceAttributedItem>) -> Self {
        Self { items, ..self.clone() }
    }
}

/// Canonical spacing for syntax tokens, without parsing a rendered attribute.
#[tracing::instrument(level = "debug")]
#[hotpath::measure]
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
