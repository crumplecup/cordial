//! Declaration, method and free-function lifetime contracts.
use super::{Declaration, FunctionSite, ImplSite, MethodSite, provenance, types};
use crate::etiquettes::internal_error_chain::apply::graph_apply::syntax;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs, HasGenericParams, HasName},
};
use std::collections::{BTreeMap, BTreeSet};
use tracing::instrument;

#[instrument(level = "trace", skip(item, name, field_lists))]
pub(super) fn declaration(
    item: &impl HasGenericParams,
    name: ast::Name,
    is_enum: bool,
    module_depth: usize,
    field_lists: Vec<ast::FieldList>,
) -> Declaration {
    let fields: Vec<_> = field_lists
        .into_iter()
        .flat_map(|list| match list {
            ast::FieldList::RecordFieldList(list) => list
                .fields()
                .filter_map(|field| {
                    Some((
                        field.name().map(|name| name.text().to_string()),
                        field.ty()?,
                    ))
                })
                .collect::<Vec<_>>(),
            ast::FieldList::TupleFieldList(list) => list
                .fields()
                .filter_map(|field| Some((None, field.ty()?)))
                .collect(),
        })
        .collect();
    Declaration {
        name: name.text().to_string(),
        is_enum,
        module_depth,
        ident_end: Some(name.syntax().text_range().end().into()),
        generics_empty: types::generics_empty(item),
        one_lifetime: types::one_lifetime(item),
        generics_range: types::generics_range(item),
        where_clause: item.where_clause().is_some(),
        needs_borrowed: fields
            .iter()
            .any(|(name, ty)| types::field_needs(name.as_deref(), ty, false)),
        needs_owned: fields
            .iter()
            .any(|(name, ty)| types::field_needs(name.as_deref(), ty, true)),
        retained_lifetimes: fields
            .iter()
            .filter(|(name, ty)| !types::field_needs(name.as_deref(), ty, true))
            .flat_map(|(_, ty)| types::lifetimes(ty.syntax()))
            .collect(),
    }
}

#[instrument(level = "trace", skip(item))]
pub(super) fn implementation(item: &ast::Impl, module_depth: usize) -> ImplSite {
    ImplSite {
        name: item.self_ty().and_then(syntax::type_name),
        module_depth,
        impl_token_end: item
            .impl_token()
            .map(|token| token.text_range().end().into()),
        generics_empty: types::generics_empty(item),
        one_lifetime: types::one_lifetime(item),
        generics_range: types::generics_range(item),
        trait_lifetimes: item
            .trait_()
            .map_or_else(BTreeSet::new, |ty| types::lifetimes(ty.syntax())),
        methods: item
            .assoc_item_list()
            .into_iter()
            .flat_map(|list| list.assoc_items())
            .filter_map(|item| match item {
                ast::AssocItem::Fn(function) => Some(method(&function)),
                _ => None,
            })
            .collect(),
    }
}

#[instrument(level = "trace", skip(item))]
fn method(item: &ast::Fn) -> MethodSite {
    let parameters = syntax::parameters(item);
    let output_self = item.ret_type().and_then(|ret| ret.ty()).is_some_and(|ty| matches!(ty, ast::Type::PathType(path) if path.path().is_some_and(|path| path.qualifier().is_none() && path.coloncolon_token().is_none() && path.segment().is_some_and(|segment| segment.name_ref().is_some_and(|name| name.text() == "Self") && segment.generic_arg_list().is_none()))));
    let all_inputs_typed = item
        .param_list()
        .is_some_and(|list| list.self_param().is_none());
    let one_lifetime = types::one_lifetime(item);
    let constructor_anchors = one_lifetime
        .as_ref()
        .filter(|lifetime| {
            output_self
                && all_inputs_typed
                && item
                    .param_list()
                    .into_iter()
                    .flat_map(|list| list.params())
                    .any(|param| types::lifetimes(param.syntax()).contains(*lifetime))
        })
        .cloned()
        .into_iter()
        .collect();
    let safe_constructor_lifetimes = parameters
        .iter()
        .filter(|(_, parameters)| provenance::safe_constructor(item, parameters))
        .map(|(lifetime, _)| lifetime.clone())
        .collect();
    let mut used_lifetimes: BTreeSet<_> = parameters.keys().cloned().collect();
    if let Some(body) = item.body() {
        used_lifetimes.extend(types::lifetimes(body.syntax()));
    }
    MethodSite {
        output_self,
        all_inputs_typed,
        generics_empty: types::generics_empty(item),
        one_lifetime,
        generics_range: types::generics_range(item),
        ident_end: item
            .name()
            .map(|name| name.syntax().text_range().end().into()),
        safe_constructor_lifetimes,
        constructor_anchors,
        used_lifetimes,
    }
}

#[instrument(level = "trace", skip(item))]
pub(super) fn function(item: &ast::Fn) -> Option<FunctionSite> {
    let return_names: BTreeSet<_> = item
        .ret_type()
        .into_iter()
        .flat_map(|ret| ret.syntax().descendants().collect::<Vec<_>>())
        .filter(|node| {
            !node
                .ancestors()
                .any(|node| node.kind() == ra_ap_syntax::SyntaxKind::TOKEN_TREE)
        })
        .filter_map(ast::PathType::cast)
        .filter(|ty| !types::trait_path(ty))
        .filter_map(|ty| {
            ty.path()?
                .segment()?
                .name_ref()
                .map(|name| name.text().to_string())
        })
        .collect();
    if return_names.is_empty() {
        return None;
    }
    let safe_returns: BTreeMap<_, _> = syntax::parameters(item)
        .into_iter()
        .map(|(lifetime, parameters)| {
            let names = return_names
                .iter()
                .filter(|name| provenance::safe_return(item, name, &parameters))
                .cloned()
                .collect();
            (lifetime, names)
        })
        .collect();
    Some(FunctionSite {
        return_names,
        safe_returns,
    })
}
