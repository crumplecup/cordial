//! Unresolved source and Kind edges from canonical database field types.
use super::{ErrorEdgeEvidence, ErrorEdgeField, ErrorEdgeStorage};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs},
};

impl ErrorEdgeEvidence {
    #[tracing::instrument(level = "trace", skip(file, ty, field))]
    pub(in crate::etiquettes::internal_error_chain::apply) fn collect(
        file: &DatabaseSource<'_>,
        ty: &ast::Type,
        field: ErrorEdgeField,
    ) -> Option<Self> {
        let (storage, type_path) = match ty {
            ast::Type::RefType(reference) if reference.mut_token().is_none() => (
                ErrorEdgeStorage::Borrowed {
                    lifetime: reference
                        .lifetime()?
                        .syntax()
                        .text()
                        .to_string()
                        .trim_start_matches('\'')
                        .to_string(),
                },
                plain_path(&reference.ty()?)?,
            ),
            ast::Type::PathType(path) if is_box(&path.path()?) => {
                let args: Vec<_> = path
                    .path()?
                    .segment()?
                    .generic_arg_list()?
                    .generic_args()
                    .collect();
                let [ast::GenericArg::TypeArg(inner)] = args.as_slice() else {
                    return None;
                };
                (ErrorEdgeStorage::Boxed, plain_path(&inner.ty()?)?)
            }
            _ => (ErrorEdgeStorage::Owned, plain_path(ty)?),
        };
        Some(Self {
            field,
            storage,
            type_path,
            type_text: file.text(ty.syntax()).to_string(),
            target: None,
        })
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
fn plain_path(ty: &ast::Type) -> Option<Vec<String>> {
    let ast::Type::PathType(ty) = ty else {
        return None;
    };
    let mut path = ty.path();
    let mut parts = Vec::new();
    while let Some(current) = path {
        let segment = current.segment()?;
        if segment.type_anchor().is_some()
            || segment.parenthesized_arg_list().is_some()
            || segment.generic_arg_list().is_some_and(|args| {
                args.generic_args()
                    .any(|arg| !matches!(arg, ast::GenericArg::LifetimeArg(_)))
            })
        {
            return None;
        }
        parts.push(segment.name_ref()?.text().to_string());
        path = current.qualifier();
    }
    parts.reverse();
    Some(parts)
}

#[tracing::instrument(level = "trace", skip(path))]
fn is_box(path: &ast::Path) -> bool {
    let mut current = Some(path.clone());
    let mut parts = Vec::new();
    while let Some(path) = current {
        let Some(name) = path.segment().and_then(|part| part.name_ref()) else {
            return false;
        };
        parts.push(name.text().to_string());
        current = path.qualifier();
    }
    matches!(parts.as_slice(), [name] if name == "Box")
        || matches!(parts.as_slice(), [name, module, root] if name == "Box" && module == "boxed" && matches!(root.as_str(), "std" | "alloc"))
}
