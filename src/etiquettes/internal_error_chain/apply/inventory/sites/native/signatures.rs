//! Written paths, return contracts, imports and standard Result alias guards.
use super::{ErrorTryEvidence, ErrorTryScope, PathFact};
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasGenericArgs, HasGenericParams, HasModuleItem, HasName},
};
use std::collections::BTreeSet;

#[tracing::instrument(level = "trace", skip(value))]
pub(super) fn path(value: &ast::Path) -> PathFact {
    let mut parts = Vec::new();
    let mut qualified = false;
    let mut current = Some(value.clone());
    let mut generic = false;
    let mut leading = false;
    while let Some(value) = current {
        if let Some(segment) = value.segment() {
            generic |=
                segment.generic_arg_list().is_some() || segment.parenthesized_arg_list().is_some();
            if let Some(name) = segment.name_ref() {
                parts.push(name.text().to_string());
            }
            if let Some(anchor) = segment.type_anchor() {
                qualified = true;
                if anchor.as_token().is_some()
                    && let Some(path) = anchor
                        .syntax()
                        .children()
                        .filter_map(ast::PathType::cast)
                        .last()
                        .and_then(|ty| ty.path())
                {
                    let mut names = self::path(&path).names;
                    names.reverse();
                    parts.extend(names);
                }
            }
            if value.qualifier().is_none() {
                leading =
                    value.coloncolon_token().is_some() || segment.coloncolon_token().is_some();
            }
        }
        current = value.qualifier();
    }
    parts.reverse();
    let bare = parts.len() == 1 && !leading && !generic && !qualified;
    PathFact::new(parts, bare, qualified)
}
#[tracing::instrument(level = "trace", skip(ty))]
pub(super) fn plain_path(ty: &ast::Type) -> Option<Vec<String>> {
    let ast::Type::PathType(ty) = ty else {
        return None;
    };
    let value = ty.path()?;
    let fact = path(&value);
    if fact.qualified || fact.names.is_empty() {
        return None;
    }
    let mut current = Some(value);
    while let Some(value) = current {
        let segment = value.segment()?;
        if segment.parenthesized_arg_list().is_some()
            || segment.generic_arg_list().is_some_and(|args| {
                args.generic_args()
                    .any(|arg| !matches!(arg, ast::GenericArg::LifetimeArg(_)))
            })
        {
            return None;
        }
        current = value.qualifier();
    }
    Some(fact.names)
}
#[tracing::instrument(level = "trace", skip(path))]
pub(super) fn single_type_argument(path: &ast::Path) -> Option<ast::Type> {
    let args: Vec<_> = path.segment()?.generic_arg_list()?.generic_args().collect();
    let [ast::GenericArg::TypeArg(ty)] = args.as_slice() else {
        return None;
    };
    ty.ty()
}
#[tracing::instrument(level = "trace", skip(output, scope))]
pub(super) fn return_evidence(
    output: Option<&ast::Type>,
    scope: ErrorTryScope,
) -> ErrorTryEvidence {
    let value = output.and_then(|ty| match ty {
        ast::Type::PathType(ty) => ty.path(),
        _ => None,
    });
    let value = value.filter(|value| !path(value).qualified);
    let names = value.as_ref().map(|value| path(value).names);
    let error = value
        .as_ref()
        .filter(|value| {
            path(value)
                .names
                .last()
                .is_some_and(|name| name == "Result")
        })
        .and_then(|value| value.segment()?.generic_arg_list())
        .and_then(|args| {
            let args: Vec<_> = args.generic_args().collect();
            if args.len() != 2 {
                return None;
            }
            match &args[1] {
                ast::GenericArg::TypeArg(ty) => plain_path(&ty.ty()?),
                _ => None,
            }
        });
    ErrorTryEvidence::new(scope, names, error)
}
#[tracing::instrument(level = "trace", skip(node))]
pub(super) fn mentions(node: &SyntaxNode) -> BTreeSet<String> {
    if matches!(node.kind(), ra_ap_syntax::SyntaxKind::TOKEN_TREE) {
        return BTreeSet::new();
    }
    let mut names = ast::PathType::cast(node.clone())
        .filter(|_| is_type_path(node))
        .and_then(|ty| ty.path())
        .map(|value| path(&value).names.into_iter().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    for child in node.children() {
        names.extend(mentions(&child));
    }
    names
}
#[tracing::instrument(level = "trace", skip(tree, names, glob))]
pub(super) fn imports(tree: &ast::UseTree, names: &mut BTreeSet<String>, glob: &mut bool) {
    if let Some(value) = tree.path() {
        names.extend(path(&value).names);
    }
    if let Some(rename) = tree.rename() {
        if let Some(name) = rename.name() {
            names.insert(name.text().to_string());
        } else if rename.underscore_token().is_some() {
            names.insert("_".to_string());
        }
    }
    *glob |= tree.star_token().is_some();
    if let Some(list) = tree.use_tree_list() {
        for child in list.use_trees() {
            imports(&child, names, glob);
        }
    }
}
#[tracing::instrument(level = "trace", skip(file, alias))]
pub(super) fn stable_alias(file: &DatabaseSource<'_>, alias: &ast::TypeAlias) -> Option<String> {
    let ast::Type::PathType(ty) = alias.ty()? else {
        return None;
    };
    let value = ty.path()?;
    let fact = path(&value);
    if fact.qualified {
        return None;
    }
    let root = match fact.names.as_slice() {
        [name] if name == "Result" => "Result",
        [root, module, name]
            if matches!(root.as_str(), "std" | "core")
                && module == "result"
                && name == "Result" =>
        {
            root
        }
        _ => return None,
    };
    if alias.generic_param_list().is_some_and(|list| {
        list.generic_params().any(|param| matches!(param, ast::GenericParam::TypeParam(param) if param.name().is_some_and(|name| name.text() == root)))
    }) || file.tree().items().any(|item| shadows(&item, root))
    {
        return None;
    }
    let mut qualifier = value.qualifier();
    while let Some(value) = qualifier {
        let segment = value.segment()?;
        if segment.generic_arg_list().is_some() || segment.parenthesized_arg_list().is_some() {
            return None;
        }
        qualifier = value.qualifier();
    }
    let args: Vec<_> = value
        .segment()?
        .generic_arg_list()?
        .generic_args()
        .collect();
    let [ast::GenericArg::TypeArg(_), ast::GenericArg::TypeArg(error)] = args.as_slice() else {
        return None;
    };
    let ast::Type::PathType(error) = error.ty()? else {
        return None;
    };
    let error = path(&error.path()?);
    error.bare.then(|| error.names[0].clone())
}
#[tracing::instrument(level = "trace", skip(item))]
fn shadows(item: &ast::Item, name: &str) -> bool {
    match item {
        ast::Item::Use(item) => item.use_tree().is_some_and(|tree| binds(&tree, name)),
        ast::Item::TypeAlias(item) => item.name().is_some_and(|value| value.text() == name),
        ast::Item::Struct(item) => item.name().is_some_and(|value| value.text() == name),
        ast::Item::Enum(item) => item.name().is_some_and(|value| value.text() == name),
        ast::Item::Union(item) => item.name().is_some_and(|value| value.text() == name),
        ast::Item::Trait(item) => item.name().is_some_and(|value| value.text() == name),
        ast::Item::Module(item) => item.name().is_some_and(|value| value.text() == name),
        ast::Item::ExternCrate(item) => item
            .rename()
            .map(|rename| rename.name().is_some_and(|value| value.text() == name))
            .unwrap_or_else(|| item.name_ref().is_some_and(|value| value.text() == name)),
        ast::Item::MacroCall(_) | ast::Item::MacroRules(_) | ast::Item::MacroDef(_) => true,
        _ => false,
    }
}
#[tracing::instrument(level = "trace", skip(tree))]
fn binds(tree: &ast::UseTree, name: &str) -> bool {
    if let Some(list) = tree.use_tree_list() {
        return list.use_trees().any(|tree| binds(&tree, name));
    }
    if tree.star_token().is_some() {
        return true;
    }
    if let Some(rename) = tree.rename() {
        return rename.name().is_some_and(|value| value.text() == name);
    }
    tree.path()
        .and_then(|path| path.segment())
        .and_then(|segment| segment.name_ref())
        .is_some_and(|value| value.text() == name || value.text() == "self")
}

/// Trait bounds, impl headers and `as Trait` paths are Paths in syn, not TypePaths.
#[tracing::instrument(level = "trace", skip(node))]
pub(super) fn is_type_path(node: &SyntaxNode) -> bool {
    let Some(parent) = node.parent() else {
        return true;
    };
    if ast::TypeBound::can_cast(parent.kind()) {
        return false;
    }
    if ast::Impl::cast(parent.clone())
        .and_then(|item| item.trait_())
        .is_some_and(|ty| ty.syntax() == node)
    {
        return false;
    }
    if let Some(anchor) = ast::TypeAnchor::cast(parent) {
        return anchor.ty().is_some_and(|ty| ty.syntax() == node);
    }
    true
}
