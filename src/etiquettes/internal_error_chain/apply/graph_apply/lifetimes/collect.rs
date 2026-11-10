//! Lifetime closure evidence from the canonical database syntax.
use super::{ConditionalScope, GraphLifetimeFacts, Scope, sites, types};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, ast};
use tracing::instrument;

impl GraphLifetimeFacts {
    #[instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "GraphLifetimeFacts")]
    pub(crate) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let mut facts = Self::default();
        collect(&mut facts, file.tree().syntax(), &Scope::default());
        facts.type_paths.sort_by_key(|site| site.range);
        facts
    }
}

#[instrument(level = "trace", skip(facts, node, scope))]
fn collect(facts: &mut GraphLifetimeFacts, node: &SyntaxNode, scope: &Scope) {
    use ast::{HasGenericParams, HasName};
    if matches!(
        node.kind(),
        SyntaxKind::TOKEN_TREE
            | SyntaxKind::CFG_META
            | SyntaxKind::CFG_ATTR_META
            | SyntaxKind::UNSAFE_META
    ) {
        return;
    }
    let mut scope = scope.clone();
    if ast::Module::can_cast(node.kind()) {
        scope.module_depth += 1;
    }
    if let Some(item) = ast::Struct::cast(node.clone())
        && let Some(name) = item.name()
    {
        facts.declarations.push(sites::declaration(
            &item,
            name.clone(),
            false,
            scope.module_depth,
            item.field_list().into_iter().collect(),
        ));
        scope.conditional = Some(ConditionalScope {
            owner: name.text().to_string(),
            borrowed_only: false,
        });
    }
    if let Some(item) = ast::Enum::cast(node.clone())
        && let Some(name) = item.name()
    {
        let fields = item
            .variant_list()
            .into_iter()
            .flat_map(|list| list.variants())
            .filter_map(|variant| variant.field_list())
            .collect();
        facts.declarations.push(sites::declaration(
            &item,
            name.clone(),
            true,
            scope.module_depth,
            fields,
        ));
        scope.conditional = Some(ConditionalScope {
            owner: name.text().to_string(),
            borrowed_only: false,
        });
    }
    if let Some(item) = ast::Impl::cast(node.clone()) {
        let site = sites::implementation(&item, scope.module_depth);
        scope.lifetimes = site.one_lifetime.clone().into_iter().collect();
        scope.conditional = site
            .name
            .clone()
            .filter(|_| site.generics_empty)
            .map(|owner| ConditionalScope {
                owner,
                borrowed_only: true,
            });
        facts.impls.push(site);
    }
    if let Some(item) = ast::Fn::cast(node.clone()) {
        let associated = item.syntax().parent().is_some_and(|parent| {
            ast::AssocItemList::can_cast(parent.kind())
                || ast::ExternItemList::can_cast(parent.kind())
        });
        if !associated {
            if let Some(site) = sites::function(&item) {
                facts.functions.push(site);
            }
            scope.lifetimes = item
                .generic_param_list()
                .into_iter()
                .flat_map(|list| list.generic_params())
                .filter_map(|param| match param {
                    ast::GenericParam::LifetimeParam(param) => {
                        param.lifetime().map(|value| types::lifetime_name(&value))
                    }
                    _ => None,
                })
                .collect();
            scope.conditional = None;
        }
    }
    if let Some(path) = ast::PathType::cast(node.clone())
        && !types::trait_path(&path)
        && let Some(site) = types::path_site(&path, &scope)
    {
        facts.type_paths.push(site);
    }
    if let Some(path) = ast::PathExpr::cast(node.clone()).and_then(|expr| expr.path()) {
        use ast::HasGenericArgs;
        let mut next = Some(path);
        while let Some(path) = next {
            if let Some(segment) = path.segment() {
                if (segment.generic_arg_list().is_some()
                    || segment.parenthesized_arg_list().is_some())
                    && let Some(name) = segment.name_ref()
                {
                    facts.expr_path_blockers.insert(name.text().to_string());
                }
                if let Some(anchor) = segment.type_anchor()
                    && anchor.as_token().is_some()
                    && let Some(trait_path) = anchor
                        .syntax()
                        .children()
                        .filter_map(ast::PathType::cast)
                        .last()
                        .and_then(|ty| ty.path())
                {
                    next = Some(trait_path);
                    continue;
                }
            }
            next = path.qualifier();
        }
    }
    for child in node.children() {
        collect(facts, &child, &scope);
    }
}
