//! Classify a function into [`FunctionRole`] and [`FunctionComplexity`].

use std::collections::HashSet;

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    Block, ExprIf, ExprMatch, FnArg, GenericArgument, Pat, PathArguments, ReturnType, Signature,
    Stmt, Type, TypePath,
};

use crate::config::ModularityThresholds;

use super::types::{FnContext, FunctionComplexity, FunctionKind, FunctionRole};

use tracing::instrument;
/// Classify `ident` (unqualified) from its signature, kind, and optional body.
#[instrument(level = "debug", skip(kind))]
pub fn classify(
    ident: &str,
    sig: &Signature,
    kind: FunctionKind,
    body: Option<&Block>,
) -> FnContext {
    let peek = body.map(peek_body).unwrap_or_default();
    let body_lines = body.map(block_lines).unwrap_or(1);
    let returns_result = returns_result_ty(sig);
    let returns_self = returns_self_ty(sig);
    let returns_bool = returns_named(sig, "bool");
    let role = classify_role(
        ident,
        sig,
        kind,
        &peek,
        returns_self,
        returns_bool,
        returns_result,
    );
    let complexity = classify_complexity(body_lines, returns_result, &peek);
    FnContext {
        role,
        complexity,
        param_names: param_names(sig),
        unrecordable_params: unrecordable_params(sig),
        returns_result,
        returns_self,
        return_unrecordable: return_type_unrecordable(sig),
        body_lines,
        has_error_path_event: peek.has_error_path_event,
    }
}

fn classify_role(
    ident: &str,
    sig: &Signature,
    kind: FunctionKind,
    peek: &BodyPeek,
    returns_self: bool,
    returns_bool: bool,
    returns_result: bool,
) -> FunctionRole {
    if is_constructor(ident, returns_self) {
        return FunctionRole::Constructor;
    }
    if is_getter(ident, sig, peek, returns_result) {
        return FunctionRole::Getter;
    }
    if is_setter(ident, sig) {
        return FunctionRole::Setter;
    }
    if is_predicate(ident, returns_bool) {
        return FunctionRole::Predicate;
    }
    if has_prefix(ident, &["scan_", "walk_", "visit_", "collect_"]) {
        return FunctionRole::Scan;
    }
    if ident == "ensure_dirs" || has_prefix(ident, &["load_", "read_", "write_", "fetch_"]) {
        return FunctionRole::Io;
    }
    if is_render(ident) {
        return FunctionRole::Render;
    }
    if kind == FunctionKind::TraitImplMethod {
        return FunctionRole::TraitSurface;
    }
    if is_entry(ident) {
        return FunctionRole::Entry;
    }
    FunctionRole::Other
}

fn is_constructor(ident: &str, returns_self: bool) -> bool {
    matches!(ident, "new" | "try_new" | "default")
        || ((ident == "from" || ident.starts_with("from_")) && returns_self)
}

fn is_getter(ident: &str, sig: &Signature, peek: &BodyPeek, returns_result: bool) -> bool {
    if returns_result {
        return false;
    }
    let Some(recv) = sig.receiver() else {
        return false;
    };
    if recv.reference.is_none() || recv.mutability.is_some() {
        return false;
    }
    if getter_name(ident) {
        return true;
    }
    peek.stmts <= 2 && !peek.has_branch
}

fn getter_name(ident: &str) -> bool {
    ident.starts_with("as_")
        || ident.starts_with("to_")
        || ident == "id"
        || ident.ends_with("_dir")
        || ident.ends_with("_path")
        || ident.ends_with("_name")
}

fn is_render(ident: &str) -> bool {
    ident.starts_with("render_")
        || ident.ends_with("_summary")
        || ident.ends_with("_checklist")
        || ident.ends_with("_csv")
}

fn is_entry(ident: &str) -> bool {
    ident == "main" || ident == "run" || ident.starts_with("run_")
}

fn is_setter(ident: &str, sig: &Signature) -> bool {
    if ident.starts_with("set_") || ident.starts_with("with_") {
        return true;
    }
    let Some(recv) = sig.receiver() else {
        return false;
    };
    recv.reference.is_none() && recv.mutability.is_some() && sig.inputs.len() >= 2
}

fn is_predicate(ident: &str, returns_bool: bool) -> bool {
    returns_bool
        && (has_prefix(ident, &["is_", "has_", "can_", "contains_"]) || ident.starts_with("eq_"))
}

fn has_prefix(ident: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| ident.starts_with(prefix))
}

fn classify_complexity(
    body_lines: u32,
    returns_result: bool,
    peek: &BodyPeek,
) -> FunctionComplexity {
    let hotspot_floor = ModularityThresholds::default().function_inventory_min_lines;
    if body_lines >= hotspot_floor {
        return FunctionComplexity::Hotspot;
    }
    if returns_result {
        return FunctionComplexity::Fallible;
    }
    if peek.has_branch {
        return FunctionComplexity::Branchy;
    }
    if peek.stmts <= 2 {
        return FunctionComplexity::Trivial;
    }
    FunctionComplexity::Linear
}

fn param_names(sig: &Signature) -> Vec<String> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Receiver(_) => Some("self".to_string()),
            FnArg::Typed(pat) => match &*pat.pat {
                Pat::Ident(ident) => Some(ident.ident.to_string()),
                _ => None,
            },
        })
        .collect()
}

fn unrecordable_params(sig: &Signature) -> Vec<String> {
    let generics = type_param_names(sig);
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat) => {
                let Pat::Ident(ident) = &*pat.pat else {
                    return None;
                };
                if type_is_unrecordable(&pat.ty) || type_is_generic_param(&pat.ty, &generics) {
                    Some(ident.ident.to_string())
                } else {
                    None
                }
            }
            FnArg::Receiver(_) => None,
        })
        .collect()
}

fn type_param_names(sig: &Signature) -> HashSet<String> {
    sig.generics
        .type_params()
        .map(|param| param.ident.to_string())
        .collect()
}

fn return_type_unrecordable(sig: &Signature) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => type_is_unrecordable(ty),
        ReturnType::Default => false,
    }
}

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
        _ => false,
    }
}

fn path_is_unrecordable(path: &TypePath) -> bool {
    path.path
        .segments
        .iter()
        .any(|segment| match &segment.arguments {
            PathArguments::None => false,
            PathArguments::Parenthesized(_) => true,
            PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| match arg {
                GenericArgument::Type(inner) => type_is_unrecordable(inner),
                GenericArgument::AssocType(assoc) => type_is_unrecordable(&assoc.ty),
                _ => false,
            }),
        })
}

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

fn returns_named(sig: &Signature, name: &str) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => type_path_contains(ty, name),
        ReturnType::Default => false,
    }
}

fn returns_result_ty(sig: &Signature) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => type_is_result(ty),
        ReturnType::Default => false,
    }
}

fn type_is_result(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => path.segments.last().is_some_and(|segment| {
            let ident = segment.ident.to_string();
            ident == "Result" || ident.ends_with("Result")
        }),
        Type::Reference(reference) => type_is_result(&reference.elem),
        Type::Paren(paren) => type_is_result(&paren.elem),
        Type::Group(group) => type_is_result(&group.elem),
        _ => false,
    }
}

fn returns_self_ty(sig: &Signature) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => type_is_self(ty),
        ReturnType::Default => false,
    }
}

fn type_is_self(ty: &Type) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => path.is_ident("Self"),
        Type::Reference(reference) => type_is_self(&reference.elem),
        Type::Paren(paren) => type_is_self(&paren.elem),
        Type::Group(group) => type_is_self(&group.elem),
        _ => false,
    }
}

fn type_path_contains(ty: &Type, name: &str) -> bool {
    match ty {
        Type::Path(TypePath { path, .. }) => path
            .segments
            .iter()
            .any(|segment| segment.ident == name || type_args_contain(&segment.arguments, name)),
        Type::Reference(reference) => type_path_contains(&reference.elem, name),
        Type::Paren(paren) => type_path_contains(&paren.elem, name),
        Type::Group(group) => type_path_contains(&group.elem, name),
        Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .any(|inner| type_path_contains(inner, name)),
        _ => false,
    }
}

fn type_args_contain(args: &syn::PathArguments, name: &str) -> bool {
    let syn::PathArguments::AngleBracketed(args) = args else {
        return false;
    };
    args.args.iter().any(|arg| match arg {
        syn::GenericArgument::Type(ty) => type_path_contains(ty, name),
        _ => false,
    })
}

fn block_lines(block: &Block) -> u32 {
    let start = block.span().start().line as u32;
    let end = block.span().end().line as u32;
    end.saturating_sub(start).max(1)
}

#[derive(Debug, Default)]
struct BodyPeek {
    stmts: usize,
    has_branch: bool,
    has_error_path_event: bool,
}

fn peek_body(block: &Block) -> BodyPeek {
    let stmts = block
        .stmts
        .iter()
        .filter(|stmt| !matches!(stmt, Stmt::Item(_)))
        .count();
    let mut peek = BodyPeek {
        stmts,
        has_branch: false,
        has_error_path_event: false,
    };
    BodyPeekVisitor(&mut peek).visit_block(block);
    peek
}

struct BodyPeekVisitor<'a>(&'a mut BodyPeek);

impl<'ast> Visit<'ast> for BodyPeekVisitor<'_> {
    fn visit_expr_match(&mut self, node: &'ast ExprMatch) {
        self.0.has_branch = true;
        syn::visit::visit_expr_match(self, node);
    }

    fn visit_expr_if(&mut self, node: &'ast ExprIf) {
        self.0.has_branch = true;
        syn::visit::visit_expr_if(self, node);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.0.has_branch = true;
        syn::visit::visit_expr_while(self, node);
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        self.0.has_branch = true;
        syn::visit::visit_expr_for_loop(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if is_error_or_warn_macro(&node.path) {
            self.0.has_error_path_event = true;
        }
        syn::visit::visit_macro(self, node);
    }
}

fn is_error_or_warn_macro(path: &syn::Path) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == "error" || segment.ident == "warn")
}
