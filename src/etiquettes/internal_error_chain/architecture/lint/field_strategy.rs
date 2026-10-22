//! Field storage policy for types that implement `Error`.

use syn::{GenericArgument, PathArguments, Type};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;
use crate::error::CordialResult;
use crate::etiquettes::internal_error_chain::type_graph::type_label;
use crate::etiquettes::internal_error_chain::types::{
    InternalErrorComplianceFinding, InternalErrorComplianceId,
};

use super::super::catalog::Catalog;
use super::{FindingSite, SiteClassifier};

impl Catalog {
    #[instrument(level = "debug", skip(self, strategy, findings))]
    pub(super) fn emit_field_strategy_findings(
        &self,
        strategy: ErrorFieldStrategy,
        findings: &mut Vec<InternalErrorComplianceFinding>,
    ) -> CordialResult<()> {
        if strategy == ErrorFieldStrategy::Owned {
            return Ok(());
        }
        for field in self.fields() {
            let is_error_architecture = self.impls_error(field.owner())
                || self
                    .enums()
                    .get(field.owner())
                    .is_some_and(|item| self.is_error_kind(item));
            if !is_error_architecture || field.name() == "kind" {
                continue;
            }
            let issue = if field.name() == "file" && !is_static_str(field.ty()) {
                Some("store the call-site file as `&'static str` from `Location::caller().file()`")
            } else if is_borrowed_string(field.ty()) {
                Some("borrow string contents as `&str`, not `&String`")
            } else if !is_copy(field.ty(), self.copy_impls()) {
                Some(
                    "borrow this non-Copy field; thread a lifetime through the error type as needed",
                )
            } else {
                None
            };
            let Some(issue) = issue else { continue };
            findings.push(self.finding(
                InternalErrorComplianceId::FieldStrategy001,
                FindingSite::new(
                    field.context().clone(),
                    field.file().clone(),
                    field.line(),
                    format!("{}: {} — {issue}", field.name(), type_label(field.ty())),
                ),
                SiteClassifier::default(),
            )?);
        }
        Ok(())
    }
}

#[instrument(level = "trace", skip(ty))]
fn is_static_str(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            reference.mutability.is_none()
                && reference
                    .lifetime
                    .as_ref()
                    .is_some_and(|lifetime| lifetime.ident == "static")
                && matches!(&*reference.elem, Type::Path(path) if path.path.is_ident("str"))
        }
        Type::Paren(paren) => is_static_str(&paren.elem),
        Type::Group(group) => is_static_str(&group.elem),
        _ => false,
    }
}

#[instrument(level = "trace", skip(ty))]
fn is_borrowed_string(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => {
            matches!(&*reference.elem, Type::Path(path) if path.path.segments.last().is_some_and(|seg| seg.ident == "String"))
                || is_borrowed_string(&reference.elem)
        }
        Type::Paren(paren) => is_borrowed_string(&paren.elem),
        Type::Group(group) => is_borrowed_string(&group.elem),
        Type::Array(array) => is_borrowed_string(&array.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(is_borrowed_string),
        Type::Path(path) => path.path.segments.iter().any(|segment| {
            let PathArguments::AngleBracketed(args) = &segment.arguments else {
                return false;
            };
            args.args.iter().any(|arg| match arg {
                GenericArgument::Type(inner) => is_borrowed_string(inner),
                _ => false,
            })
        }),
        _ => false,
    }
}

#[instrument(level = "trace", skip(ty, local_copy))]
fn is_copy(ty: &Type, local_copy: &std::collections::BTreeSet<String>) -> bool {
    match ty {
        Type::Reference(_) | Type::Ptr(_) | Type::Never(_) => true,
        Type::Paren(paren) => is_copy(&paren.elem, local_copy),
        Type::Group(group) => is_copy(&group.elem, local_copy),
        Type::Tuple(tuple) => tuple.elems.iter().all(|ty| is_copy(ty, local_copy)),
        Type::Array(array) => is_copy(&array.elem, local_copy),
        Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return false;
            };
            let name = segment.ident.to_string();
            if matches!(
                name.as_str(),
                "bool"
                    | "char"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "f32"
                    | "f64"
            ) {
                return true;
            }
            if local_copy.contains(&name) {
                return match &segment.arguments {
                    PathArguments::None => true,
                    PathArguments::AngleBracketed(args) => args.args.iter().all(|arg| match arg {
                        GenericArgument::Type(inner) => is_copy(inner, local_copy),
                        GenericArgument::Lifetime(_) | GenericArgument::Const(_) => true,
                        _ => false,
                    }),
                    _ => false,
                };
            }
            if !matches!(name.as_str(), "Option" | "Result" | "PhantomData") {
                return false;
            }
            let PathArguments::AngleBracketed(args) = &segment.arguments else {
                return false;
            };
            args.args.iter().all(|arg| match arg {
                GenericArgument::Type(inner) => is_copy(inner, local_copy),
                GenericArgument::Lifetime(_) | GenericArgument::Const(_) => true,
                _ => false,
            })
        }
        _ => false,
    }
}
