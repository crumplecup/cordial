//! Syntactic evidence for an error field conversion unit.
//!
//! Every site is tied to a shared parsed source snapshot. Rust name resolution
//! and macro expansion are outside this scan; those paths remain explicitly
//! unresolved instead of being treated as absent.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::instrument;
mod batch;
mod collection;
mod graph;
mod helpers;

pub(crate) use graph::{conversion_units_from_ir, enrich_conversion_units};

/// A source construct that may participate in converting an error field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorConversionSiteKind {
    /// A struct expression that builds the error.
    Constructor,
    /// A derive or attribute macro that may synthesize a constructor or use.
    GeneratedConstructor,
    /// A direct call to an associated constructor.
    Caller,
    /// An explicit use of the error type in a type position.
    TypeUse,
    /// A read or write of the candidate field.
    FieldUse,
    /// A struct pattern that binds the candidate field.
    FieldPattern,
    /// A `From` implementation involving the error type.
    FromConversion,
    /// A `?` expression in a function returning the error type.
    TryConversion,
    /// A `source` or `kind` field in the error declaration.
    ErrorEdge,
    /// A type alias, import, or associated method value.
    Alias,
    /// Tokens hidden behind a macro boundary.
    Macro,
    /// A reference in another source file whose target cannot be resolved.
    CrossFileReference,
}

/// One located construction, use, or unresolved path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ErrorConversionSite {
    path: PathBuf,
    #[getter(copy)]
    line: usize,
    #[getter(copy)]
    column: usize,
    #[getter(copy)]
    kind: ErrorConversionSiteKind,
    /// Exact declared type text for a `source` or `kind` edge, when available.
    related_type: Option<String>,
    #[getter(copy)]
    resolved: bool,
}

/// All visible evidence for one private error field in a crate target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ErrorConversionUnit {
    declaration: PathBuf,
    type_name: String,
    field_name: String,
    #[getter(copy)]
    name_ambiguous: bool,
    sites: Vec<ErrorConversionSite>,
}

/// A field identified once from its declaration before site collection.
struct ConversionCandidate {
    declaration: PathBuf,
    type_name: String,
    field_name: String,
    line: u32,
    name_ambiguous: bool,
}

impl ErrorConversionUnit {
    /// Whether every recorded path is understood well enough to plan a unit.
    #[instrument(level = "trace", skip(self))]
    pub fn is_closed(&self) -> bool {
        self.sites.iter().all(|site| site.resolved)
            && self
                .sites
                .iter()
                .any(|site| site.kind == ErrorConversionSiteKind::Constructor)
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn blocks_existing_field_edit(&self) -> bool {
        self.sites.iter().any(|site| {
            (!site.resolved && site.path != self.declaration)
                || (!site.resolved
                    && matches!(
                        site.kind,
                        ErrorConversionSiteKind::FieldUse
                            | ErrorConversionSiteKind::FieldPattern
                            | ErrorConversionSiteKind::Alias
                            | ErrorConversionSiteKind::Macro
                            | ErrorConversionSiteKind::CrossFileReference
                            | ErrorConversionSiteKind::Constructor
                            | ErrorConversionSiteKind::GeneratedConstructor
                    ))
        })
    }
}
