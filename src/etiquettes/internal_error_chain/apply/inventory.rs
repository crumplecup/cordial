//! Syntactic evidence for an error field conversion unit.
//!
//! Every site is tied to a shared parsed source snapshot. Rust name resolution
//! and macro expansion are outside this scan; those paths remain explicitly
//! unresolved instead of being treated as absent.

use std::path::PathBuf;

use super::Edit;
use super::initializers::FieldKind;
use crate::config::ErrorFieldStrategy;
use serde::{Deserialize, Serialize};
use tracing::instrument;
mod batch;
mod collection;
mod edges;
mod from;
mod graph;
mod helpers;
mod resolution;
mod source_method;
mod try_conversion;

pub use edges::{
    ErrorEdgeEvidence, ErrorEdgeField, ErrorEdgeStorage, ErrorEdgeTarget, ErrorEdgeTargetKind,
};
pub use from::ErrorFromEvidence;
pub(crate) use graph::{conversion_units_from_ir, enrich_conversion_units};
pub use source_method::ErrorSourceMethodEvidence;
pub use try_conversion::{ErrorTryEvidence, ErrorTryScope};

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
    /// An override of the error trait's `source` method.
    SourceMethod,
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
    /// Typed declaration facts for a supported `source` or `kind` edge.
    edge: Option<ErrorEdgeEvidence>,
    /// Written type paths on a `From` implementation; not name-resolved.
    from_conversion: Option<ErrorFromEvidence>,
    /// Written signature and self-field uses in an error source override.
    source_method: Option<ErrorSourceMethodEvidence>,
    /// Lexical return signature at a `?` site; not type-resolved.
    try_conversion: Option<ErrorTryEvidence>,
    /// Written path to a candidate declaration, before module resolution.
    reference_path: Option<Vec<String>>,
    /// Unique declaration reached by the written path, when proven.
    target: Option<ErrorEdgeTarget>,
    #[getter(copy)]
    resolved: bool,
}

impl ErrorConversionSite {
    /// Exact declared type text for a supported `source` or `kind` edge.
    #[instrument(level = "trace", skip(self))]
    pub fn related_type(&self) -> Option<String> {
        self.edge.as_ref().map(|edge| edge.type_text().clone())
    }
}

/// All visible evidence for one private error field in a crate target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct ErrorConversionUnit {
    declaration: PathBuf,
    #[getter(copy)]
    source_digest: [u8; 32],
    type_name: String,
    field_name: String,
    #[getter(copy)]
    name_ambiguous: bool,
    #[getter(skip)]
    field: FieldEvidence,
    #[getter(skip)]
    initializer: InitializerEvidence,
    #[getter(skip)]
    lifetime: LifetimeEvidence,
    sites: Vec<ErrorConversionSite>,
}

/// A proven initializer edit for the storage direction implied by this source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct InitializerEvidence {
    target: ErrorFieldStrategy,
    edits: Option<Vec<Edit>>,
}

/// A closed same-file lifetime migration proven against the shared tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LifetimeEvidence {
    target: ErrorFieldStrategy,
    edits: Option<Vec<Edit>>,
}

/// Declaration facts that the edit planner needs without revisiting syntax.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(super) struct FieldEvidence {
    #[getter(copy)]
    kind: FieldKind,
    storage: StringStorage,
    struct_lifetime: Option<String>,
    #[getter(copy)]
    private: bool,
    #[getter(copy)]
    unique_struct: bool,
    #[getter(copy)]
    type_range: Option<(usize, usize)>,
    #[getter(copy)]
    lifetime_candidate: bool,
}

/// The source storage shape, independent of the configured target strategy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum StringStorage {
    Owned,
    Borrowed {
        lifetime: Option<String>,
        mutable: bool,
    },
}

/// A field identified once from its declaration before site collection.
struct ConversionCandidate {
    declaration: PathBuf,
    source_digest: [u8; 32],
    type_name: String,
    field_name: String,
    line: u32,
    name_ambiguous: bool,
    field: FieldEvidence,
    initializer: InitializerEvidence,
    initializer_kind: FieldKind,
    initializer_lifetime: Option<String>,
}

impl ErrorConversionUnit {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn field_evidence(&self) -> &FieldEvidence {
        &self.field
    }
    #[instrument(level = "trace", skip(self, strategy))]
    pub(super) fn lifetime_candidate(&self, strategy: ErrorFieldStrategy) -> bool {
        self.field.lifetime_candidate && self.initializer.target == strategy
    }
    #[instrument(level = "trace", skip(self, strategy))]
    pub(super) fn initializer_edits(&self, strategy: ErrorFieldStrategy) -> Option<Vec<Edit>> {
        (self.initializer.target == strategy)
            .then(|| self.initializer.edits.clone())
            .flatten()
    }
    #[instrument(level = "trace", skip(self, strategy))]
    pub(super) fn lifetime_edits(&self, strategy: ErrorFieldStrategy) -> Option<Vec<Edit>> {
        (self.lifetime.target == strategy)
            .then(|| self.lifetime.edits.clone())
            .flatten()
    }
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
