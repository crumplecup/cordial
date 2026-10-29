//! Resolve local aliases for field-storage assessment.

use std::collections::BTreeMap;
use tracing::instrument;

/// Recognize the actual local alias target during architecture collection.
/// The marker grants no policy exemption: the target must be static str.
#[instrument(level = "trace", skip(items))]
pub(super) fn static_aliases(items: &[syn::Item]) -> BTreeMap<String, syn::Type> {
    let mut aliases = BTreeMap::<String, Vec<&syn::ItemType>>::new();
    for item in items {
        if let syn::Item::Type(item) = item {
            aliases
                .entry(item.ident.to_string())
                .or_default()
                .push(item);
        }
    }
    aliases
        .into_iter()
        .filter_map(|(name, items)| {
            let [item] = items.as_slice() else {
                return None;
            };
            let syn::Type::Reference(reference) = &*item.ty else { return None };
            (item.generics.params.is_empty()
                && item.attrs.iter().all(|attr| {
                    attr.path().is_ident("doc") || attr.path().is_ident("cfg")
                })
                && reference.mutability.is_none()
                && reference.lifetime.as_ref().is_some_and(|lifetime| lifetime.ident == "static")
                && matches!(&*reference.elem, syn::Type::Path(path) if path.qself.is_none() && path.path.is_ident("str")))
            .then(|| (name, (*item.ty).clone()))
        })
        .collect()
}
