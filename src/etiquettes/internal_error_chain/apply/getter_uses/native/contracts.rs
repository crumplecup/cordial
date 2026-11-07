//! Existing conservative method contracts, collected before identity resolution.
use super::super::{MethodContract, ReceiverRecipe};
use crate::loader::DatabaseSource;
use ra_ap_syntax::ast::{self, HasAttrs, HasGenericArgs, HasGenericParams, HasModuleItem, HasName};
use tracing::instrument;

#[instrument(level = "debug", skip(file))]
pub(super) fn methods(file: &DatabaseSource<'_>) -> Vec<MethodContract> {
    let mut methods = Vec::new();
    for item in file.tree().items() {
        match item {
            ast::Item::Impl(item)
                if item.trait_().is_none()
                    && item
                        .generic_param_list()
                        .is_none_or(|list| list.generic_params().next().is_none())
                    && !conditional(&item) =>
            {
                let owner = item
                    .self_ty()
                    .map_or(ReceiverRecipe::Unknown, |ty| ty_recipe(file, &ty));
                for method in item
                    .assoc_item_list()
                    .into_iter()
                    .flat_map(|list| list.assoc_items())
                {
                    if let ast::AssocItem::Fn(method) = method
                        && method.name().is_some_and(|name| name.text() == "file")
                        && !conditional(&method)
                    {
                        methods.push(MethodContract {
                            owner: owner.clone(),
                            has_receiver: method
                                .param_list()
                                .is_some_and(|list| list.self_param().is_some()),
                        });
                    }
                }
            }
            ast::Item::Struct(item)
                if super::super::super::getters::has_native_derive(
                    file,
                    &item,
                    "derive_getters",
                    "Getters",
                ) =>
            {
                let field = match item.field_list() {
                    Some(ast::FieldList::RecordFieldList(list)) => list.fields().any(|field| {
                        field.name().is_some_and(|name| name.text() == "file")
                            && !field
                                .attrs()
                                .any(|attr| attr.simple_name().as_deref() == Some("getter"))
                    }),
                    _ => false,
                };
                if field && let Some(name) = item.name() {
                    methods.push(MethodContract {
                        owner: ReceiverRecipe::Declaration(name.text().to_string()),
                        has_receiver: true,
                    });
                }
            }
            _ => {}
        }
    }
    methods
}

#[instrument(level = "trace", skip(file, ty))]
pub(super) fn ty_recipe(file: &DatabaseSource<'_>, ty: &ast::Type) -> ReceiverRecipe {
    match ty {
        ast::Type::RefType(value) => value
            .ty()
            .map_or(ReceiverRecipe::Unknown, |ty| ty_recipe(file, &ty)),
        ast::Type::PathType(value) => value
            .path()
            .map_or(ReceiverRecipe::Unknown, |path| path_recipe(file, &path)),
        _ => ReceiverRecipe::Unknown,
    }
}

#[instrument(level = "trace", skip(file, path))]
pub(super) fn path_recipe(file: &DatabaseSource<'_>, path: &ast::Path) -> ReceiverRecipe {
    let Some((names, arguments)) = path_parts(path) else {
        return ReceiverRecipe::Unknown;
    };
    if matches!(names.as_slice(), [root, panic, location] if matches!(root.as_str(), "std" | "core") && panic == "panic" && location == "Location")
    {
        return if unshadowed(file, &names[0]) {
            ReceiverRecipe::Location
        } else {
            ReceiverRecipe::Unknown
        };
    }
    if arguments {
        ReceiverRecipe::Unknown
    } else {
        ReceiverRecipe::Path(names)
    }
}

#[instrument(level = "trace", skip(path))]
pub(super) fn path_parts(path: &ast::Path) -> Option<(Vec<String>, bool)> {
    let mut names = Vec::new();
    let mut arguments = false;
    let mut next = Some(path.clone());
    while let Some(path) = next {
        let segment = path.segment()?;
        if segment.type_anchor().is_some() {
            return None;
        }
        names.push(segment.name_ref()?.text().to_string());
        arguments |=
            segment.generic_arg_list().is_some() || segment.parenthesized_arg_list().is_some();
        next = path.qualifier();
    }
    names.reverse();
    Some((names, arguments))
}

#[instrument(level = "trace", skip(file))]
pub(super) fn unshadowed(file: &DatabaseSource<'_>, name: &str) -> bool {
    !file.tree().items().any(|item| match item {
        ast::Item::Module(item) => item.name().is_some_and(|n| n.text() == name),
        ast::Item::Struct(item) => item.name().is_some_and(|n| n.text() == name),
        ast::Item::Enum(item) => item.name().is_some_and(|n| n.text() == name),
        ast::Item::TypeAlias(item) => item.name().is_some_and(|n| n.text() == name),
        ast::Item::ExternCrate(item) => item
            .rename()
            .map(|r| r.name().is_some_and(|n| n.text() == name))
            .unwrap_or_else(|| item.name_ref().is_some_and(|n| n.text() == name)),
        ast::Item::Use(item) => item.use_tree().is_some_and(|tree| shadows(&tree, name)),
        _ => false,
    })
}

#[instrument(level = "trace", skip(tree))]
fn shadows(tree: &ast::UseTree, name: &str) -> bool {
    if let Some(list) = tree.use_tree_list() {
        return list.use_trees().any(|tree| shadows(&tree, name));
    }
    tree.star_token().is_some()
        || tree
            .rename()
            .map(|r| {
                r.name().is_some_and(|n| n.text() == name)
                    || (name == "_" && r.underscore_token().is_some())
            })
            .unwrap_or_else(|| {
                tree.path()
                    .and_then(|path| path.segment())
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|n| n.text() == name)
            })
}

#[instrument(level = "trace", skip(item))]
fn conditional(item: &impl HasAttrs) -> bool {
    item.attrs().any(|attr| {
        if let Some(ast::Meta::CfgAttrMeta(meta)) = attr.meta() {
            let nested: Vec<_> = meta.metas().collect();
            return meta.cfg_predicate().is_none() || nested.is_empty() || nested.iter().any(|meta| meta.path().and_then(|path| path_parts(&path)).is_none_or(|(path, _)| path != ["tracing", "instrument"]));
        }
        let path = attr.path().and_then(|path| path_parts(&path)).map(|(path, _)| path);
        !matches!(path.as_deref(), Some([name]) if matches!(name.as_str(), "doc" | "inline" | "track_caller" | "cold" | "must_use" | "deprecated")) && path.as_deref() != Some(&["tracing".to_string(), "instrument".to_string()])
    })
}
