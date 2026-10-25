//! Typed declaration facts for source and Kind edges.

use std::path::PathBuf;

use syn::spanned::Spanned;
use syn::{GenericArgument, PathArguments, Type};
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::super::span_range;
use super::helpers::plain_type_path;

/// Which relationship a field on an error declaration represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorEdgeField {
    /// A nested cause exposed through `Error::source`.
    Source,
    /// The error's category or variant payload.
    Kind,
}

/// The syntactic storage shape of a `source` or `kind` field.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorEdgeStorage {
    /// A value stored directly in the field.
    Owned,
    /// A value stored behind `Box`.
    Boxed,
    /// A reference with an explicit lifetime.
    Borrowed {
        /// Lifetime named on the reference.
        lifetime: String,
    },
}

/// The kind of top-level declaration reached by a local error edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorEdgeTargetKind {
    /// A struct declaration.
    Struct,
    /// An enum declaration.
    Enum,
}

/// Identity and source snapshot of a private same-file declaration candidate.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_getters::Getters,
)]
pub struct ErrorEdgeTarget {
    declaration: PathBuf,
    #[getter(copy)]
    line: usize,
    #[getter(copy)]
    source_digest: [u8; 32],
    #[getter(copy)]
    kind: ErrorEdgeTargetKind,
}

impl ErrorEdgeTarget {
    #[instrument(level = "debug", skip(file, kind))]
    pub(super) fn from_shared_file(
        file: &SharedParsedFile,
        line: usize,
        kind: ErrorEdgeTargetKind,
    ) -> Self {
        Self {
            declaration: file.path().to_path_buf(),
            line,
            source_digest: file.source_digest(),
            kind,
        }
    }
}

/// Parsed edge facts from the shared source tree; the path is not yet resolved.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    derive_getters::Getters,
    derive_setters::Setters,
)]
#[setters(generate = false, borrow_self)]
pub struct ErrorEdgeEvidence {
    #[getter(copy)]
    field: ErrorEdgeField,
    storage: ErrorEdgeStorage,
    /// Path segments for the stored value, before import or alias resolution.
    type_path: Vec<String>,
    /// Exact spelling of the declared type for diagnostics.
    type_text: String,
    /// Unique, private top-level declaration candidate in the same source file.
    #[setters(generate, rename = "set_target", strip_option)]
    target: Option<ErrorEdgeTarget>,
}

#[instrument(level = "debug", skip(file, field, edge_field))]
pub(super) fn error_edge_evidence(
    file: &SharedParsedFile,
    field: &syn::Field,
    edge_field: ErrorEdgeField,
) -> Option<ErrorEdgeEvidence> {
    let (start, end) = span_range(file.source(), field.ty.span())?;
    let (storage, type_path) = match &field.ty {
        Type::Reference(reference) if reference.mutability.is_none() => (
            ErrorEdgeStorage::Borrowed {
                lifetime: reference.lifetime.as_ref()?.ident.to_string(),
            },
            plain_type_path(&reference.elem)?,
        ),
        Type::Path(path) if path.qself.is_none() && is_box_path(&path.path) => {
            let argument = match &path.path.segments.last()?.arguments {
                PathArguments::AngleBracketed(arguments) if arguments.args.len() == 1 => {
                    arguments.args.first()?
                }
                _ => return None,
            };
            let GenericArgument::Type(inner) = argument else {
                return None;
            };
            (ErrorEdgeStorage::Boxed, plain_type_path(inner)?)
        }
        _ => (ErrorEdgeStorage::Owned, plain_type_path(&field.ty)?),
    };
    Some(ErrorEdgeEvidence {
        field: edge_field,
        storage,
        type_path,
        type_text: file.source()[start..end].to_string(),
        target: None,
    })
}

#[instrument(level = "trace", skip(path))]
fn is_box_path(path: &syn::Path) -> bool {
    let names: Vec<_> = path
        .segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect();
    matches!(names.as_slice(), [name] if name == "Box")
        || matches!(names.as_slice(), [root, module, name]
            if matches!(root.as_str(), "std" | "alloc") && module == "boxed" && name == "Box")
}
