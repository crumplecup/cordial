//! Predicates for ABI-mandated parameters and cfg sibling functions.

use std::collections::{HashMap, HashSet};

use syn::{Attribute, FnArg, ImplItem, ImplItemFn, Item, ItemFn, Pat, Signature};

use tracing::instrument;

/// Whether `attrs` marks a function whose parameter list is a compiler-
/// mandated ABI, not a shape its author chose: `#[proc_macro_attribute]`
/// (always exactly `(TokenStream, TokenStream)`), `#[proc_macro_derive]`,
/// or `#[proc_macro]`. Same reasoning as skipping a foreign trait impl's
/// signature -- the parameter list isn't this function's own to shrink --
/// just enforced by the macro system instead of a trait declaration.
#[instrument(level = "trace", skip(attrs), ret)]
pub(in crate::etiquettes::antipatterns::scan) fn has_proc_macro_abi_attr(
    attrs: &[syn::Attribute],
) -> bool {
    ["proc_macro_attribute", "proc_macro_derive", "proc_macro"]
        .iter()
        .any(|name| attrs.iter().any(|attr| attr.path().is_ident(name)))
}

/// Whether `attrs` + `block` together mark a Creusot `#[trusted]
/// #[logic(opaque)]` axiom stub: an uninterpreted logic function whose
/// parameters exist only to make the axiom parametric across call sites
/// (Pearlite substitutes the caller's own expression for each one), never
/// to be read by the body -- `dead` is Creusot's own sentinel body for
/// exactly this idiom, confirmed against real sites in both this
/// workspace and `elicitation_creusot::logic_fns.rs`. Checking both the
/// attribute and the body shape (not either alone) keeps this narrow:
/// `#[logic(opaque)]` alone doesn't guarantee a `dead` body, and a
/// function that merely happens to reference an identifier named `dead`
/// without the attribute isn't this pattern.
#[instrument(level = "trace", skip(attrs, block))]
pub(in crate::etiquettes::antipatterns::scan) fn is_creusot_opaque_logic_stub(
    attrs: &[syn::Attribute],
    block: &syn::Block,
) -> bool {
    let has_logic_opaque = attrs.iter().any(|attr| {
        let syn::Meta::List(list) = &attr.meta else {
            return false;
        };
        list.path.is_ident("logic") && list.tokens.to_string().replace(' ', "") == "opaque"
    });
    if !has_logic_opaque {
        return false;
    }
    matches!(
        block.stmts.as_slice(),
        [syn::Stmt::Expr(syn::Expr::Path(expr_path), None)] if expr_path.path.is_ident("dead")
    )
}

#[instrument(level = "trace", skip(attrs), ret)]
fn has_cfg_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("cfg"))
}

/// The non-underscore-prefixed top-level named parameters in `sig` --
/// tuple/struct-destructured params are skipped, since a name-based
/// cfg-sibling match (below) only makes sense against a plain identifier.
#[instrument(level = "trace", skip(sig))]
fn real_param_names(sig: &Signature) -> impl Iterator<Item = String> + '_ {
    sig.inputs.iter().filter_map(|arg| {
        let FnArg::Typed(pat_type) = arg else {
            return None;
        };
        let Pat::Ident(pat_ident) = pat_type.pat.as_ref() else {
            return None;
        };
        let name = pat_ident.ident.to_string();
        (!name.starts_with('_')).then_some(name)
    })
}

/// For each function name appearing more than once among `items`, where
/// *every* occurrence carries some `#[cfg(...)]` attribute, the union of
/// every non-underscore-prefixed parameter name used by any occurrence.
///
/// Rust requires same-named items in one scope to have disjoint `cfg`
/// gates to coexist at all -- finding ≥2 cfg-gated occurrences of one
/// name in the same item list is already proof they're deliberate
/// variants of "the same" function (e.g. one body for `#[cfg(kani)]`,
/// another for `#[cfg(not(kani))`), not a chance name collision. A
/// parameter underscore-prefixed in one variant but present, unprefixed,
/// under the same name in a sibling is genuinely read there -- the
/// underscore only reflects that one branch's own body, not the
/// function's real shape across every branch.
#[instrument(level = "debug", skip(items), ret)]
pub(in crate::etiquettes::antipatterns::scan) fn cfg_sibling_real_param_names_in_items(
    items: &[Item],
) -> HashMap<String, HashSet<String>> {
    let mut groups: HashMap<String, Vec<&ItemFn>> = HashMap::new();
    for item in items {
        if let Item::Fn(item_fn) = item {
            groups
                .entry(item_fn.sig.ident.to_string())
                .or_default()
                .push(item_fn);
        }
    }
    groups
        .into_iter()
        .filter(|(_, fns)| fns.len() >= 2 && fns.iter().all(|f| has_cfg_attr(&f.attrs)))
        .map(|(name, fns)| {
            let real_names = fns.iter().flat_map(|f| real_param_names(&f.sig)).collect();
            (name, real_names)
        })
        .collect()
}

/// [`cfg_sibling_real_param_names_in_items`], for one `impl` block's
/// associated functions instead of a module's free functions.
#[instrument(level = "debug", skip(items), ret)]
pub(in crate::etiquettes::antipatterns::scan) fn cfg_sibling_real_param_names_in_impl_items(
    items: &[ImplItem],
) -> HashMap<String, HashSet<String>> {
    let mut groups: HashMap<String, Vec<&ImplItemFn>> = HashMap::new();
    for item in items {
        if let ImplItem::Fn(item_fn) = item {
            groups
                .entry(item_fn.sig.ident.to_string())
                .or_default()
                .push(item_fn);
        }
    }
    groups
        .into_iter()
        .filter(|(_, fns)| fns.len() >= 2 && fns.iter().all(|f| has_cfg_attr(&f.attrs)))
        .map(|(name, fns)| {
            let real_names = fns.iter().flat_map(|f| real_param_names(&f.sig)).collect();
            (name, real_names)
        })
        .collect()
}
