//! Visibility evidence collected from the loader's shared syntax.
use super::super::visibility::{
    VisibilityBody, VisibilityChild, VisibilityChildBody, VisibilityKind,
};
use super::visibility_vis::{is_cfg_test, is_doc_hidden, item_vis, leaf_name_count, vis_kind};
use crate::{CordialResult, loader::SharedParsedFile};
use syn::{Item, spanned::Spanned};

#[tracing::instrument(level = "debug", skip(file), err(level = "warn"))]
pub(crate) fn collect(file: &SharedParsedFile) -> CordialResult<VisibilityBody> {
    Ok(items(&file.syntax().items))
}

#[tracing::instrument(level = "debug", skip(input))]
fn items(input: &[Item]) -> VisibilityBody {
    let (mut leaf_pub, mut leaf_crate, mut children) = (0, 0, Vec::new());
    for item in input {
        match item {
            Item::Mod(module) => {
                if is_cfg_test(&module.attrs) || is_doc_hidden(&module.attrs) {
                    continue;
                }
                let content = if let Some((_, nested)) = &module.content {
                    VisibilityChildBody::Inline(items(nested))
                } else {
                    VisibilityChildBody::External(path_attr(&module.attrs))
                };
                children.push(VisibilityChild::new(
                    module.ident.to_string(),
                    module.span().start().line as u32,
                    vis_kind(&module.vis),
                    content,
                ));
            }
            Item::Macro(invocation) => {
                let (public, internal) = macro_names(invocation);
                leaf_pub += public;
                leaf_crate += internal;
            }
            _ => {
                let n = leaf_name_count(item);
                match item_vis(item) {
                    Some(VisibilityKind::Pub) => {
                        leaf_pub += n;
                        leaf_crate += n;
                    }
                    Some(VisibilityKind::PubCrate) => leaf_crate += n,
                    _ => {}
                }
            }
        }
    }
    VisibilityBody::new(leaf_pub, leaf_crate, children)
}

#[tracing::instrument(level = "trace", skip(attrs))]
fn path_attr(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        if let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
        {
            return Some(s.value());
        }
        None
    })
}

#[tracing::instrument(level = "trace", skip(invocation))]
fn macro_names(invocation: &syn::ItemMacro) -> (usize, usize) {
    #[cfg(feature = "verus_ir")]
    if invocation.mac.path.is_ident("verus") {
        return crate::verus_ir::count_verus_item_names(invocation.mac.tokens.clone());
    }
    let _ = invocation;
    (0, 0)
}
