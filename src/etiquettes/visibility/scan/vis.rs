//! Visibility kind helpers for scanned items.

use syn::{Attribute, Item, UseTree, Visibility};

use tracing::instrument;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VisKind {
    Private,
    Pub,
    PubCrate,
    PubSuper,
}

impl VisKind {
    #[instrument(level = "debug", skip(self))]
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Pub => "pub",
            Self::PubCrate => "pub(crate)",
            Self::PubSuper => "pub(super)",
        }
    }

    #[instrument(level = "trace", skip(self), ret)]
    pub(super) fn is_unrestricted_pub(self) -> bool {
        matches!(self, Self::Pub)
    }
}

#[instrument(level = "debug", skip(vis))]
pub(super) fn vis_kind(vis: &Visibility) -> VisKind {
    match vis {
        Visibility::Public(_) => VisKind::Pub,
        Visibility::Inherited => VisKind::Private,
        Visibility::Restricted(restricted) => {
            if restricted.path.is_ident("crate") {
                VisKind::PubCrate
            } else if restricted.path.is_ident("super") {
                VisKind::PubSuper
            } else {
                VisKind::PubCrate
            }
        }
    }
}

#[instrument(level = "debug", skip(item))]
pub(super) fn item_vis(item: &Item) -> Option<VisKind> {
    let vis = match item {
        Item::Const(item) => &item.vis,
        Item::Enum(item) => &item.vis,
        Item::Fn(item) => &item.vis,
        Item::Static(item) => &item.vis,
        Item::Struct(item) => &item.vis,
        Item::Trait(item) => &item.vis,
        Item::TraitAlias(item) => &item.vis,
        Item::Type(item) => &item.vis,
        Item::Use(item) => &item.vis,
        Item::Union(item) => &item.vis,
        Item::ForeignMod(_) | Item::Impl(_) | Item::Macro(_) | Item::Verbatim(_) | Item::Mod(_) => {
            return None;
        }
        _ => return None,
    };
    Some(vis_kind(vis))
}

#[instrument(level = "debug", skip(item))]
pub(super) fn leaf_name_count(item: &Item) -> usize {
    match item {
        Item::Use(item) => use_name_count(&item.tree),
        Item::Const(_)
        | Item::Enum(_)
        | Item::Fn(_)
        | Item::Static(_)
        | Item::Struct(_)
        | Item::Trait(_)
        | Item::TraitAlias(_)
        | Item::Type(_)
        | Item::Union(_) => 1,
        _ => 0,
    }
}

#[instrument(level = "debug", skip(tree))]
fn use_name_count(tree: &UseTree) -> usize {
    match tree {
        UseTree::Name(_) | UseTree::Rename(_) => 1,
        UseTree::Glob(_) => 0,
        UseTree::Path(path) => use_name_count(&path.tree),
        UseTree::Group(group) => group.items.iter().map(use_name_count).sum(),
    }
}

#[instrument(level = "trace", skip(attrs), ret)]
pub(super) fn is_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        list.path.is_ident("cfg") && list.tokens.to_string().replace(' ', "") == "test"
    })
}

#[instrument(level = "trace", skip(attrs))]
pub(super) fn is_doc_hidden(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("doc") {
            return false;
        }
        match &attr.meta {
            syn::Meta::List(list) => list.tokens.to_string().replace(' ', "").contains("hidden"),
            _ => false,
        }
    })
}
