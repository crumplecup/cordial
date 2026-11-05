//! Static field contracts retain their source shape, independent of policy.
use super::{AntipatternCandidate, AntipatternRuleId, Collector, Context, StaticField, types};
use crate::{
    CordialResult,
    etiquettes::antipatterns::{catalog::StaticFieldShape, scan::truncate_snippet},
};
use ra_ap_syntax::{
    AstNode, ast,
    ast::{HasGenericArgs, HasName},
};
use std::collections::HashSet;

impl Collector<'_, '_> {
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(super) fn fields(
        &mut self,
        fields: Option<ast::FieldList>,
        owner: &str,
        context: &Context,
    ) -> CordialResult<()> {
        let fields: Vec<_> = match fields {
            Some(ast::FieldList::RecordFieldList(list)) => list
                .fields()
                .filter_map(|field| Some((field.name()?.text().to_string(), field.ty()?)))
                .collect(),
            Some(ast::FieldList::TupleFieldList(list)) => list
                .fields()
                .enumerate()
                .filter_map(|(i, field)| Some((format!("_{i}"), field.ty()?)))
                .collect(),
            None => Vec::new(),
        };
        let type_name = owner.split("::").next().unwrap_or(owner);
        for (name, ty) in fields {
            if !disallowed(&ty, &self.facts.traits) {
                continue;
            }
            let location = is_location(&ty);
            if !location
                && self.facts.constants.contains(type_name)
                && !self.facts.runtime.contains(type_name)
            {
                continue;
            }
            let shape = if location {
                StaticFieldShape::Location
            } else if static_string(&ty) {
                StaticFieldShape::String
            } else {
                StaticFieldShape::Other
            };
            let mut parts = context.modules.clone();
            parts.extend([owner.into(), name]);
            let site = self.site(
                AntipatternRuleId::StructStaticRef001,
                parts.join("::"),
                self.source.line(ty.syntax().text_range().start()),
                truncate_snippet(&label(&ty), 96),
            )?;
            self.sites
                .push(AntipatternCandidate::StaticField(StaticField::new(
                    site,
                    self.facts.errors.contains(type_name),
                    shape,
                )));
        }
        Ok(())
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
fn static_ref(ty: &ast::RefType) -> bool {
    ty.lifetime()
        .is_some_and(|lifetime| lifetime.syntax().text().to_string() == "'static")
}

#[tracing::instrument(level = "trace", skip(ty, traits))]
fn local_spine(ty: &ast::Type, traits: &HashSet<String>) -> bool {
    match ty {
        ast::Type::DynTraitType(object) => types::bounds(object)
            .iter()
            .any(|name| traits.contains(name)),
        ast::Type::RefType(reference) => {
            static_ref(reference)
                && reference
                    .ty()
                    .as_ref()
                    .is_some_and(|ty| local_spine(ty, traits))
        }
        ast::Type::ParenType(inner) => inner
            .ty()
            .as_ref()
            .is_some_and(|ty| local_spine(ty, traits)),
        ast::Type::ArrayType(inner) => inner
            .ty()
            .as_ref()
            .is_some_and(|ty| local_spine(ty, traits)),
        ast::Type::SliceType(inner) => inner
            .ty()
            .as_ref()
            .is_some_and(|ty| local_spine(ty, traits)),
        _ => false,
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
fn children(ty: &ast::Type) -> Vec<ast::Type> {
    match ty {
        ast::Type::PathType(_) => types::path(ty)
            .into_iter()
            .flat_map(|path| {
                let mut segments = Vec::new();
                let mut current = Some(path);
                while let Some(path) = current {
                    segments.extend(path.segment());
                    current = path.qualifier();
                }
                segments
            })
            .flat_map(|segment| {
                let mut args = segment
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|list| list.generic_args())
                    .filter_map(|arg| match arg {
                        ast::GenericArg::TypeArg(arg) => arg.ty(),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if let Some(list) = segment.parenthesized_arg_list() {
                    args.extend(list.type_args().filter_map(|arg| arg.ty()));
                }
                args
            })
            .collect(),
        ast::Type::DynTraitType(object) => object
            .type_bound_list()
            .into_iter()
            .flat_map(|list| list.bounds())
            .filter_map(|bound| bound.ty())
            .flat_map(|ty| children(&ty))
            .collect(),
        ast::Type::RefType(inner) => inner.ty().into_iter().collect(),
        ast::Type::ParenType(inner) => inner.ty().into_iter().collect(),
        ast::Type::ArrayType(inner) => inner.ty().into_iter().collect(),
        ast::Type::SliceType(inner) => inner.ty().into_iter().collect(),
        ast::Type::PtrType(inner) => inner.ty().into_iter().collect(),
        ast::Type::TupleType(inner) => inner.fields().collect(),
        _ => Vec::new(),
    }
}

#[tracing::instrument(level = "trace", skip(ty, traits))]
fn disallowed(ty: &ast::Type, traits: &HashSet<String>) -> bool {
    if let ast::Type::RefType(reference) = ty
        && static_ref(reference)
        && reference
            .ty()
            .as_ref()
            .is_none_or(|inner| !local_spine(inner, traits))
    {
        return true;
    }
    children(ty).iter().any(|inner| disallowed(inner, traits))
}

#[tracing::instrument(level = "trace", skip(ty))]
fn is_location(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::RefType(inner) => inner.ty().as_ref().is_some_and(is_location),
        ast::Type::ParenType(inner) => inner.ty().as_ref().is_some_and(is_location),
        _ => types::path(ty)
            .as_ref()
            .and_then(types::path_name)
            .is_some_and(|name| name == "Location"),
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
fn static_string(ty: &ast::Type) -> bool {
    if let ast::Type::RefType(reference) = ty
        && static_ref(reference)
        && reference
            .ty()
            .as_ref()
            .is_some_and(|ty| types::label(ty) == "str")
    {
        return true;
    }
    !matches!(ty, ast::Type::DynTraitType(_)) && children(ty).iter().any(static_string)
}

#[tracing::instrument(level = "trace", skip(ty))]
fn label(ty: &ast::Type) -> String {
    match ty {
        ast::Type::RefType(reference) => format!(
            "&{}{} {}",
            reference
                .lifetime()
                .map(|l| l.syntax().text().to_string())
                .unwrap_or_default(),
            if reference.mut_token().is_some() {
                " mut"
            } else {
                ""
            },
            reference
                .ty()
                .as_ref()
                .map(label)
                .unwrap_or_else(|| "?".into())
        ),
        ast::Type::PathType(_) => {
            let mut segments = Vec::new();
            let mut path = types::path(ty);
            while let Some(current) = path {
                if let Some(segment) = current.segment() {
                    let mut name = segment
                        .name_ref()
                        .map(|name| name.text().to_string())
                        .unwrap_or_else(|| "?".into());
                    let args: Vec<_> = segment
                        .generic_arg_list()
                        .into_iter()
                        .flat_map(|list| list.generic_args())
                        .filter_map(|arg| match arg {
                            ast::GenericArg::TypeArg(arg) => arg.ty().as_ref().map(label),
                            ast::GenericArg::LifetimeArg(arg) => {
                                arg.lifetime().map(|l| l.syntax().text().to_string())
                            }
                            _ => None,
                        })
                        .collect();
                    if !args.is_empty() {
                        name = format!("{name}<{}>", args.join(", "));
                    }
                    segments.push(name);
                }
                path = current.qualifier();
            }
            segments.reverse();
            segments.join("::")
        }
        ast::Type::ArrayType(inner) => format!(
            "[{}; …]",
            inner.ty().as_ref().map(label).unwrap_or_else(|| "?".into())
        ),
        ast::Type::SliceType(inner) => format!(
            "[{}]",
            inner.ty().as_ref().map(label).unwrap_or_else(|| "?".into())
        ),
        ast::Type::TupleType(inner) => format!(
            "({})",
            inner
                .fields()
                .map(|ty| label(&ty))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ast::Type::ParenType(inner) => inner.ty().as_ref().map(label).unwrap_or_else(|| "?".into()),
        _ => types::label(ty),
    }
}
