//! Static field contracts retain their source shape, independent of policy.
use super::{AntipatternRuleId, Collector, Context, PendingCandidate, StaticFieldShape, types};
use crate::{CordialResult, etiquettes::antipatterns::scan::truncate_snippet};
use ra_ap_syntax::{
    AstNode, ast,
    ast::{HasGenericArgs, HasName},
};

impl Collector<'_, '_> {
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure]
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
            // What the facts decide is kept with the candidate: with no `&'static` in the type
            // there is nothing for them to decide.
            let references = static_references(&ty);
            if references.is_empty() {
                continue;
            }
            let location = is_location(&ty);
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
            self.sites.push(PendingCandidate::Field {
                site,
                shape,
                type_name: type_name.to_owned(),
                location,
                references,
            });
        }
        Ok(())
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
fn static_ref(ty: &ast::RefType) -> bool {
    ty.lifetime()
        .is_some_and(|lifetime| lifetime.syntax().text() == "'static")
}

/// The traits a type names along its spine: through `&'static`, parentheses, arrays and slices to a
/// trait object, whose bounds they are.
#[tracing::instrument(level = "trace", skip(ty))]
fn spine_names(ty: &ast::Type) -> Vec<String> {
    match ty {
        ast::Type::DynTraitType(object) => types::bounds(object),
        ast::Type::RefType(reference) => {
            if static_ref(reference) {
                reference.ty().as_ref().map(spine_names).unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        ast::Type::ParenType(inner) => inner.ty().as_ref().map(spine_names).unwrap_or_default(),
        ast::Type::ArrayType(inner) => inner.ty().as_ref().map(spine_names).unwrap_or_default(),
        ast::Type::SliceType(inner) => inner.ty().as_ref().map(spine_names).unwrap_or_default(),
        _ => Vec::new(),
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

/// For each `&'static` in the type (the type itself and those in it), the traits its spine names, or
/// none where the reference has no type. The field is disallowed if any of them is not excused by a
/// trait the crate declares: that is decided where the crate's traits are known.
#[tracing::instrument(level = "trace", skip(ty))]
fn static_references(ty: &ast::Type) -> Vec<Option<Vec<String>>> {
    let mut found = Vec::new();
    collect_references(ty, &mut found);
    found
}

#[tracing::instrument(level = "trace", skip(ty, found))]
fn collect_references(ty: &ast::Type, found: &mut Vec<Option<Vec<String>>>) {
    if let ast::Type::RefType(reference) = ty
        && static_ref(reference)
    {
        found.push(reference.ty().as_ref().map(spine_names));
    }
    for inner in children(ty) {
        collect_references(&inner, found);
    }
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
