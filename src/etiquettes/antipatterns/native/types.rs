//! Type shapes and stable compatibility labels over canonical syntax.
use super::{AntipatternCandidate, AntipatternRuleId, Collector, Context};
use crate::{CordialResult, etiquettes::antipatterns::scan::truncate_snippet};
use ra_ap_syntax::{AstNode, ast, ast::HasGenericArgs};

#[tracing::instrument(level = "trace", skip(path))]
pub(super) fn path_name(path: &ast::Path) -> Option<String> {
    path.segment()?
        .name_ref()
        .map(|name| name.text().to_string())
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn path(ty: &ast::Type) -> Option<ast::Path> {
    match ty {
        ast::Type::PathType(ty) => ty.path(),
        _ => None,
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn label(ty: &ast::Type) -> String {
    match ty {
        ast::Type::RefType(ty) => ty.ty().as_ref().map(label),
        ast::Type::ParenType(ty) => ty.ty().as_ref().map(label),
        _ => path(ty).as_ref().and_then(path_name),
    }
    .unwrap_or_else(|| "?".into())
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn arguments(ty: &ast::Type) -> Vec<ast::Type> {
    path(ty)
        .and_then(|path| path.segment())
        .and_then(|segment| segment.generic_arg_list())
        .map(|list| {
            list.generic_args()
                .filter_map(|arg| match arg {
                    ast::GenericArg::TypeArg(arg) => arg.ty(),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn stringish(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::RefType(ty) => ty.ty().as_ref().is_some_and(stringish),
        ast::Type::ParenType(ty) => ty.ty().as_ref().is_some_and(stringish),
        _ => path(ty)
            .as_ref()
            .and_then(path_name)
            .is_some_and(|name| matches!(name.as_str(), "String" | "str")),
    }
}

#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn bounds(ty: &ast::DynTraitType) -> Vec<String> {
    ty.type_bound_list()
        .into_iter()
        .flat_map(|list| list.bounds())
        .map(|bound| {
            if let Some(lifetime) = bound.lifetime() {
                lifetime
                    .syntax()
                    .text()
                    .to_string()
                    .trim_start_matches('\'')
                    .into()
            } else if bound.use_token().is_some() {
                "use<…>".into()
            } else {
                bound.ty().as_ref().map(label).unwrap_or_else(|| "?".into())
            }
        })
        .collect()
}

impl Collector<'_, '_> {
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    pub(super) fn error_type(&mut self, ty: &ast::Type, context: &Context) -> CordialResult<()> {
        let Some(path) = path(ty) else {
            return Ok(());
        };
        let name = path_name(&path);
        let args = arguments(ty);
        let rule_snippet = match name.as_deref() {
            Some("Result") if args.get(1).is_some_and(stringish) => Some((
                AntipatternRuleId::StringError001,
                format!("Result<…, {}>", label(&args[1])),
            )),
            Some("Box") if args.len() == 1 => match &args[0] {
                ast::Type::DynTraitType(object) => {
                    let bounds = bounds(object);
                    bounds.iter().any(|name| name == "Error").then(|| {
                        (
                            AntipatternRuleId::BoxDynError001,
                            format!("Box<dyn {}>", bounds.join(" + ")),
                        )
                    })
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((rule, snippet)) = rule_snippet {
            let site = self.site(
                rule,
                context.label(),
                self.source.line(ty.syntax().text_range().start()),
                truncate_snippet(&snippet, 96),
            )?;
            self.sites.push(AntipatternCandidate::Record(site));
        }
        Ok(())
    }
}
