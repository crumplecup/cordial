//! Result aliases preserve the error type without exposing its private storage.

use syn::{GenericArgument, GenericParam, Item, ItemType, PathArguments, Type, UseTree};
use tracing::instrument;

/// Restrict this contract to the standard Result and a direct error argument.
/// Direct error aliases can construct the error and still need closure evidence.
#[instrument(level = "trace", skip(item, syntax, error))]
pub(super) fn stable_result_alias(item: &ItemType, syntax: &syn::File, error: &str) -> bool {
    let Type::Path(ty) = &*item.ty else {
        return false;
    };
    if ty.qself.is_some() {
        return false;
    }
    let segments: Vec<_> = ty.path.segments.iter().collect();
    let names: Vec<_> = segments.iter().map(|part| part.ident.to_string()).collect();
    let root = match names.as_slice() {
        [name] if name == "Result" => "Result",
        [root, module, name]
            if matches!(root.as_str(), "std" | "core")
                && module == "result"
                && name == "Result" =>
        {
            root
        }
        _ => return false,
    };
    if item
        .generics
        .params
        .iter()
        .any(|param| matches!(param, GenericParam::Type(param) if param.ident == root))
        || syntax.items.iter().any(|item| shadows(item, root))
    {
        return false;
    }
    if segments
        .iter()
        .take(segments.len() - 1)
        .any(|part| !part.arguments.is_empty())
    {
        return false;
    }
    let Some(part) = segments.last() else {
        return false;
    };
    let PathArguments::AngleBracketed(args) = &part.arguments else {
        return false;
    };
    let mut args = args.args.iter();
    matches!((args.next(), args.next(), args.next()),
        (Some(GenericArgument::Type(_)), Some(GenericArgument::Type(Type::Path(error_type))), None)
            if error_type.qself.is_none() && error_type.path.is_ident(error))
}

#[instrument(level = "trace", skip(item))]
fn shadows(item: &Item, name: &str) -> bool {
    match item {
        Item::Use(item) => import_binds(&item.tree, name),
        Item::Type(item) => item.ident == name,
        Item::Struct(item) => item.ident == name,
        Item::Enum(item) => item.ident == name,
        Item::Union(item) => item.ident == name,
        Item::Trait(item) => item.ident == name,
        Item::TraitAlias(item) => item.ident == name,
        Item::Mod(item) => item.ident == name,
        Item::ExternCrate(item) => {
            item.rename
                .as_ref()
                .map(|(_, alias)| alias)
                .unwrap_or(&item.ident)
                == name
        }
        // Expansion could introduce a shadowing type into this scope.
        Item::Macro(_) => true,
        _ => false,
    }
}

#[instrument(level = "trace", skip(tree))]
fn import_binds(tree: &UseTree, name: &str) -> bool {
    match tree {
        UseTree::Path(part) => import_binds(&part.tree, name),
        UseTree::Name(part) => part.ident == name || part.ident == "self",
        UseTree::Rename(part) => part.rename == name,
        UseTree::Group(group) => group.items.iter().any(|part| import_binds(part, name)),
        UseTree::Glob(_) => true,
    }
}
