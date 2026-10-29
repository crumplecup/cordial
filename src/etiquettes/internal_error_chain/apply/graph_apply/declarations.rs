//! Typed declaration shapes collected from the shared syntax tree.

use serde::{Deserialize, Serialize};

use super::super::inventory::{ErrorEdgeEvidence, ErrorEdgeTarget, StringStorage};
use super::Shape;

mod collect;
mod desired;
mod repairs;

type Range = (usize, usize);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(in crate::etiquettes::internal_error_chain::apply) struct GraphDeclarationFacts {
    declarations: Vec<GraphDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(in crate::etiquettes::internal_error_chain::apply) struct GraphDeclaration {
    name: String,
    #[getter(skip)]
    line: usize,
    #[getter(skip)]
    declaration_line: usize,
    #[getter(skip)]
    column: usize,
    #[getter(copy)]
    kind: DeclarationKind,
    #[getter(skip)]
    visibility: Visibility,
    #[getter(skip)]
    allowed_attrs: bool,
    #[getter(copy)]
    error_impl: bool,
    #[getter(copy)]
    derives_error: bool,
    #[getter(copy)]
    generics_empty: bool,
    #[getter(skip)]
    single_struct_lifetime: Option<String>,
    #[getter(skip)]
    only_lifetime: Option<String>,
    fields: GraphFields,
    variants: Vec<GraphVariant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::etiquettes::internal_error_chain::apply) enum DeclarationKind {
    Struct,
    Enum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Visibility {
    Inherited,
    Restricted,
    Public,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::etiquettes::internal_error_chain::apply) enum GraphFields {
    Named(Vec<GraphField>),
    Unnamed(Vec<GraphField>),
    Unit,
}

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(super) struct GraphVariant {
    name: String,
    #[getter(skip)]
    allowed_attrs: bool,
    fields: GraphFields,
}

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(in crate::etiquettes::internal_error_chain::apply) struct GraphField {
    #[getter(skip)]
    name: Option<String>,
    #[getter(copy)]
    line: usize,
    #[getter(skip)]
    type_line: usize,
    #[getter(skip)]
    visibility: Visibility,
    #[getter(skip)]
    allowed_attrs: bool,
    #[getter(skip)]
    type_name: Option<String>,
    #[getter(skip)]
    storage: Option<(Shape, String)>,
    #[getter(skip)]
    copy_shape: bool,
    #[getter(skip)]
    reference: bool,
    #[getter(skip)]
    unit_reference: bool,
    #[getter(copy)]
    type_range: Option<Range>,
    #[getter(skip)]
    borrowed_referent: Option<String>,
    #[getter(skip)]
    owned_referent: Option<String>,
    #[getter(skip)]
    stripped_lifetime: Option<(String, String)>,
    #[getter(skip)]
    source_edge: Option<ErrorEdgeEvidence>,
    #[getter(skip)]
    kind_edge: Option<ErrorEdgeEvidence>,
    #[getter(skip)]
    candidate_storage: Option<StringStorage>,
    getter_storage: Option<super::super::getters::GetterStorageFacts>,
}

impl GraphDeclarationFacts {
    #[tracing::instrument(level = "trace", skip(self, resolve))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn resolved_incoming_edges(
        &self,
        mut resolve: impl FnMut(&[String]) -> Option<ErrorEdgeTarget>,
    ) -> Vec<(String, ErrorEdgeTarget)> {
        let mut incoming = Vec::new();
        for item in &self.declarations {
            if item.kind != DeclarationKind::Struct {
                continue;
            }
            for field in item.fields.iter() {
                let edge = match field.name.as_deref() {
                    Some("source") => field.source_edge.as_ref(),
                    Some("kind") => field.kind_edge.as_ref(),
                    _ => None,
                };
                if let Some(target) = edge.and_then(|edge| resolve(edge.type_path())) {
                    incoming.push((item.name.clone(), target));
                }
            }
        }
        incoming
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn all(&self) -> &[GraphDeclaration] {
        &self.declarations
    }

    #[tracing::instrument(level = "trace", skip(self, kind))]
    pub(super) fn at(
        &self,
        line: usize,
        column: usize,
        kind: DeclarationKind,
    ) -> Option<&GraphDeclaration> {
        self.declarations
            .iter()
            .find(|item| item.line == line && item.column == column && item.kind == kind)
    }
}

impl GraphDeclaration {
    #[tracing::instrument(level = "trace", skip(self))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn is_inherited(&self) -> bool {
        self.visibility == Visibility::Inherited
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn is_public(&self) -> bool {
        self.visibility == Visibility::Public
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn attrs_allowed(&self) -> bool {
        self.allowed_attrs
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn only_lifetime(&self) -> Option<&str> {
        self.only_lifetime.as_deref()
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn single_struct_lifetime(
        &self,
    ) -> Option<&str> {
        self.single_struct_lifetime.as_deref()
    }
}

impl GraphVariant {
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn attrs_allowed(&self) -> bool {
        self.allowed_attrs
    }
}

impl GraphField {
    #[tracing::instrument(level = "trace", skip(self))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn candidate_storage(
        &self,
    ) -> Option<&StringStorage> {
        self.candidate_storage.as_ref()
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn is_inherited(&self) -> bool {
        self.visibility == Visibility::Inherited
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn is_public(&self) -> bool {
        self.visibility == Visibility::Public
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn attrs_allowed(&self) -> bool {
        self.allowed_attrs
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn type_name(&self) -> Option<&str> {
        self.type_name.as_deref()
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn storage(&self) -> Option<(Shape, &str)> {
        self.storage
            .as_ref()
            .map(|(shape, inner)| (*shape, inner.as_str()))
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn is_copy_shape(&self) -> bool {
        self.copy_shape
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn is_reference(&self) -> bool {
        self.reference
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn is_unit_reference(&self) -> bool {
        self.unit_reference
    }
    #[tracing::instrument(level = "trace", skip(self, source))]
    pub(super) fn edge(&self, source: bool) -> Option<&ErrorEdgeEvidence> {
        if source {
            self.source_edge.as_ref()
        } else {
            self.kind_edge.as_ref()
        }
    }
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn wants_change(&self, borrowed: bool) -> bool {
        borrowed != self.reference
    }
}

impl GraphFields {
    #[tracing::instrument(level = "trace", skip(self))]
    pub(super) fn iter(&self) -> impl Iterator<Item = &GraphField> {
        match self {
            Self::Named(fields) | Self::Unnamed(fields) => fields.iter(),
            Self::Unit => [].iter(),
        }
    }
}
