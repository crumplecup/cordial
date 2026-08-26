//! Which parameter and return types tracing can record without extra bounds.

use std::collections::HashSet;

use syn::visit::Visit;
use syn::{FnArg, GenericArgument, Pat, PathArguments, ReturnType, Signature, Type, TypePath};

use tracing::instrument;

#[instrument(level = "debug", skip(sig))]
pub(super) fn unrecordable_params(sig: &Signature) -> Vec<String> {
    let generics = type_param_names(sig);
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat) => Some(pat),
            FnArg::Receiver(_) => None,
        })
        .flat_map(|pat| pattern_bindings(&pat.pat, Some(&pat.ty)))
        .filter_map(|(name, ty)| {
            let unrecordable = match ty {
                Some(ty) => type_is_unrecordable(ty) || type_is_generic_param(ty, &generics),
                // Couldn't correlate this binding to a sub-type (a
                // struct/slice/or-pattern, or a tuple/reference pattern
                // whose shape doesn't match its type) -- conservatively
                // unrecordable rather than silently missing it.
                None => true,
            };
            unrecordable.then_some(name)
        })
        .collect()
}

/// Every binding name `pat` introduces, paired with the sub-type of `ty`
/// it structurally corresponds to when that's determinable -- `None`
/// when the shapes don't line up. Real motivating case: `fn ensures(
/// (actual, expected): (T, T)) -> bool` destructures one tuple-typed
/// parameter into two bindings; `tracing::instrument`'s real expansion
/// records `actual`/`expected` individually via `Debug`, not "the
/// parameter" as one opaque unit -- unrecordability has to be decided
/// per binding, zipping the tuple pattern against the tuple type
/// element-wise, not just against the top-level `Pat::Ident` case this
/// used to handle alone.
#[instrument(level = "trace", skip(pat, ty))]
pub(super) fn pattern_bindings<'a>(
    pat: &'a Pat,
    ty: Option<&'a Type>,
) -> Vec<(String, Option<&'a Type>)> {
    match (pat, ty) {
        (Pat::Ident(ident), _) => vec![(ident.ident.to_string(), ty)],
        (Pat::Tuple(pat_tuple), Some(Type::Tuple(ty_tuple)))
            if pat_tuple.elems.len() == ty_tuple.elems.len() =>
        {
            pat_tuple
                .elems
                .iter()
                .zip(ty_tuple.elems.iter())
                .flat_map(|(p, t)| pattern_bindings(p, Some(t)))
                .collect()
        }
        (Pat::Reference(pat_ref), Some(Type::Reference(ty_ref))) => {
            pattern_bindings(&pat_ref.pat, Some(&ty_ref.elem))
        }
        (Pat::Paren(pat_paren), _) => pattern_bindings(&pat_paren.pat, ty),
        _ => collect_all_idents(pat)
            .into_iter()
            .map(|name| (name, None))
            .collect(),
    }
}

/// Every `Pat::Ident` binding anywhere inside `pat`, regardless of
/// nesting shape -- the conservative fallback for pattern/type
/// combinations [`pattern_bindings`] doesn't specifically correlate
/// (struct patterns need field-type lookup this module doesn't have;
/// slice and or-patterns don't zip one-to-one with a single type at
/// all).
#[instrument(level = "trace", skip(pat))]
fn collect_all_idents(pat: &Pat) -> Vec<String> {
    struct IdentCollector(Vec<String>);
    impl<'ast> Visit<'ast> for IdentCollector {
        fn visit_pat_ident(&mut self, node: &'ast syn::PatIdent) {
            self.0.push(node.ident.to_string());
        }
    }
    let mut collector = IdentCollector(Vec::new());
    collector.visit_pat(pat);
    collector.0
}

#[instrument(level = "debug", skip(sig))]
fn type_param_names(sig: &Signature) -> HashSet<String> {
    sig.generics
        .type_params()
        .map(|param| param.ident.to_string())
        .collect()
}

#[instrument(level = "debug", skip(sig))]
pub(super) fn return_type_unrecordable(sig: &Signature) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => type_is_unrecordable(ty),
        ReturnType::Default => false,
    }
}

#[instrument(level = "debug", skip(ty))]
fn type_is_unrecordable(ty: &Type) -> bool {
    match ty {
        Type::ImplTrait(_) | Type::TraitObject(_) | Type::BareFn(_) | Type::Infer(_) => true,
        Type::Never(_) | Type::Macro(_) | Type::Verbatim(_) => true,
        Type::Reference(reference) => type_is_unrecordable(&reference.elem),
        Type::Ptr(ptr) => type_is_unrecordable(&ptr.elem),
        Type::Paren(paren) => type_is_unrecordable(&paren.elem),
        Type::Group(group) => type_is_unrecordable(&group.elem),
        Type::Slice(slice) => type_is_unrecordable(&slice.elem),
        Type::Array(array) => type_is_unrecordable(&array.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(type_is_unrecordable),
        Type::Path(path) => path_is_unrecordable(path),
        _ => true,
    }
}

/// Types tracing can record without a `Debug` bound blowing up the build.
const RECORDABLE_IDENTS: &[&str] = &[
    "bool", "char", "str", "String", "OsStr", "OsString", "Path", "PathBuf", "u8", "u16", "u32",
    "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32", "f64", "Duration",
    "Option", "Result", "Vec", "Box", "Arc", "Rc", "Cow",
];

#[instrument(level = "debug", skip(path))]
fn path_is_unrecordable(path: &TypePath) -> bool {
    let Some(last) = path.path.segments.last() else {
        return true;
    };
    if !RECORDABLE_IDENTS.contains(&last.ident.to_string().as_str()) {
        return true;
    }
    match &last.arguments {
        PathArguments::None => false,
        PathArguments::Parenthesized(_) => true,
        PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| match arg {
            GenericArgument::Type(inner) => type_is_unrecordable(inner),
            GenericArgument::AssocType(assoc) => type_is_unrecordable(&assoc.ty),
            _ => false,
        }),
    }
}

#[instrument(level = "debug", skip(sig))]
pub(super) fn return_type_borrowed(sig: &Signature) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => type_contains_borrow(ty),
        ReturnType::Default => false,
    }
}

#[instrument(level = "debug", skip(ty))]
fn type_contains_borrow(ty: &Type) -> bool {
    match ty {
        Type::Reference(_) | Type::Ptr(_) => true,
        Type::Paren(paren) => type_contains_borrow(&paren.elem),
        Type::Group(group) => type_contains_borrow(&group.elem),
        Type::Slice(slice) => type_contains_borrow(&slice.elem),
        Type::Array(array) => type_contains_borrow(&array.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(type_contains_borrow),
        Type::Path(path) => path
            .path
            .segments
            .iter()
            .any(|segment| match &segment.arguments {
                PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| match arg {
                    GenericArgument::Type(inner) => type_contains_borrow(inner),
                    GenericArgument::AssocType(assoc) => type_contains_borrow(&assoc.ty),
                    _ => false,
                }),
                _ => false,
            }),
        _ => false,
    }
}

#[instrument(level = "debug", skip(ty, generics))]
fn type_is_generic_param(ty: &Type, generics: &HashSet<String>) -> bool {
    match ty {
        Type::Path(TypePath { qself: None, path }) => path.segments.iter().any(|segment| {
            generics.contains(&segment.ident.to_string())
                || match &segment.arguments {
                    PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| match arg {
                        GenericArgument::Type(inner) => type_is_generic_param(inner, generics),
                        _ => false,
                    }),
                    _ => false,
                }
        }),
        Type::Reference(reference) => type_is_generic_param(&reference.elem, generics),
        Type::Ptr(ptr) => type_is_generic_param(&ptr.elem, generics),
        Type::Paren(paren) => type_is_generic_param(&paren.elem, generics),
        Type::Group(group) => type_is_generic_param(&group.elem, generics),
        Type::Slice(slice) => type_is_generic_param(&slice.elem, generics),
        Type::Array(array) => type_is_generic_param(&array.elem, generics),
        Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .any(|inner| type_is_generic_param(inner, generics)),
        _ => false,
    }
}
