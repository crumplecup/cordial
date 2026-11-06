//! Constructor signatures and permitted implementation edit contracts.
use super::native_syntax::{bare_type, generics_empty, one_lifetime, range, scalar, str_ref};
use crate::config::ErrorFieldStrategy;
use crate::etiquettes::internal_error_chain::apply::lifetime::ImplPlanFacts;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasGenericArgs, HasGenericParams, HasName},
};
use tracing::instrument;

#[instrument(level = "trace", skip(implementations))]
pub(super) fn constructor_lifetime(implementations: &[ast::Impl]) -> Option<String> {
    let mut found = None;
    for implementation in implementations {
        if implementation.trait_().is_some() {
            continue;
        }
        let members: Vec<_> = implementation.assoc_item_list()?.assoc_items().collect();
        let [ast::AssocItem::Fn(method)] = members.as_slice() else {
            return None;
        };
        if found.replace(one_lifetime(method)?).is_some() {
            return None;
        }
    }
    found
}

#[instrument(level = "trace", skip(method, item, strategy))]
pub(super) fn constructor_signature(
    method: &ast::Fn,
    item: &ast::Struct,
    field: &str,
    lifetime: &str,
    strategy: ErrorFieldStrategy,
) -> bool {
    if !plain_function(method)
        || !method
            .ret_type()
            .and_then(|ret| ret.ty())
            .is_some_and(|ty| bare_type(&ty, "Self"))
    {
        return false;
    }
    let valid_generics = match strategy {
        ErrorFieldStrategy::Borrowed => one_lifetime(method).as_deref() == Some(lifetime),
        ErrorFieldStrategy::Owned => generics_empty(method),
    };
    let Some(list) = method.param_list() else {
        return false;
    };
    let inputs: Vec<_> = list.params().collect();
    let Some(first) = inputs.first() else {
        return false;
    };
    let Some(ast::FieldList::RecordFieldList(fields)) = item.field_list() else {
        return false;
    };
    valid_generics
        && list.self_param().is_none()
        && first_parameter(first, field, lifetime)
        && inputs.iter().skip(1).all(|argument| {
            let Some(pattern) = parameter_pattern(argument) else {
                return false;
            };
            if pattern.ref_token().is_some() || pattern.mut_token().is_some() {
                return false;
            }
            let Some(name) = pattern.name() else {
                return false;
            };
            fields.fields().any(|candidate| {
                candidate
                    .name()
                    .is_some_and(|value| value.text() == name.text())
                    && name.text() != field
                    && candidate
                        .ty()
                        .and_then(scalar)
                        .is_some_and(|ty| argument.ty().and_then(scalar) == Some(ty))
            })
        })
}

#[instrument(level = "trace", skip(function))]
pub(super) fn plain_function(function: &ast::Fn) -> bool {
    function.attrs().next().is_none()
        && function.doc_comments().next().is_none()
        && function.async_token().is_none()
        && function.const_token().is_none()
        && function.unsafe_token().is_none()
        && function.where_clause().is_none()
        && function.body().is_none_or(|body| {
            body.attrs().next().is_none()
                && body.doc_comments().next().is_none()
                && body.stmt_list().is_none_or(|statements| {
                    statements.attrs().next().is_none()
                        && statements.doc_comments().next().is_none()
                })
        })
}

#[instrument(level = "trace", skip(parameter))]
pub(super) fn parameter_pattern(parameter: &ast::Param) -> Option<ast::IdentPat> {
    let ast::Pat::IdentPat(pattern) = parameter.pat()? else {
        return None;
    };
    pattern.pat().is_none().then_some(pattern)
}

#[instrument(level = "trace", skip(parameter))]
pub(super) fn first_parameter(parameter: &ast::Param, field: &str, lifetime: &str) -> bool {
    parameter_pattern(parameter)
        .is_some_and(|pattern| pattern.name().is_some_and(|name| name.text() == field))
        && parameter.ty().is_some_and(|ty| str_ref(&ty, lifetime))
}

#[instrument(level = "trace", skip(ty))]
pub(super) fn allowed_trait(ty: ast::Type) -> bool {
    let ast::Type::PathType(ty) = ty else {
        return false;
    };
    let mut current = ty.path();
    let mut names = Vec::new();
    while let Some(path) = current {
        let Some(segment) = path.segment() else {
            return false;
        };
        if segment.generic_arg_list().is_some()
            || segment.parenthesized_arg_list().is_some()
            || segment.type_anchor().is_some()
        {
            return false;
        }
        let Some(name) = segment.name_ref() else {
            return false;
        };
        names.push(name.text().to_string());
        current = path.qualifier();
    }
    matches!(names.as_slice(), [name, module, root] if matches!(root.as_str(), "std" | "core")
        && matches!((module.as_str(), name.as_str()), ("fmt", "Display") | ("error", "Error")))
}

#[instrument(level = "trace", skip(implementation, strategy))]
pub(super) fn impl_facts(
    implementation: &ast::Impl,
    strategy: ErrorFieldStrategy,
) -> Option<ImplPlanFacts> {
    let (constructor_generics, constructor_ident_end) = if implementation.trait_().is_none() {
        let ast::AssocItem::Fn(method) = implementation.assoc_item_list()?.assoc_items().next()?
        else {
            return None;
        };
        match strategy {
            ErrorFieldStrategy::Borrowed => {
                (Some(range(method.generic_param_list()?.syntax())), None)
            }
            ErrorFieldStrategy::Owned => (None, Some(range(method.name()?.syntax()).1)),
        }
    } else {
        (None, None)
    };
    Some(ImplPlanFacts {
        self_type: range(implementation.self_ty()?.syntax()),
        impl_token_end: (strategy == ErrorFieldStrategy::Borrowed)
            .then(|| {
                implementation
                    .impl_token()
                    .map(|token| usize::from(token.text_range().end()))
            })
            .flatten(),
        generics: (strategy == ErrorFieldStrategy::Owned)
            .then(|| {
                implementation
                    .generic_param_list()
                    .map(|list| range(list.syntax()))
            })
            .flatten(),
        constructor_generics,
        constructor_ident_end,
    })
}
