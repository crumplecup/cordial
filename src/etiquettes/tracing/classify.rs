//! Classify a function into [`FunctionRole`] and [`FunctionComplexity`].

use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Block, ExprIf, ExprMatch, FnArg, Pat, ReturnType, Signature, Stmt, Type, TypePath};

use crate::config::ModularityThresholds;

use super::types::{FnContext, FunctionComplexity, FunctionKind, FunctionRole};

/// Classify `ident` (unqualified) from its signature, kind, and optional body.
#[tracing::instrument(skip(sig, body), fields(ident, ?kind))]
pub fn classify(
    ident: &str,
    sig: &Signature,
    kind: FunctionKind,
    body: Option<&Block>,
) -> FnContext {
    let peek = body.map(peek_body).unwrap_or_default();
    let body_lines = body.map(block_lines).unwrap_or(1);
    let returns_result = returns_named(sig, "Result");
    let returns_self = returns_self_ty(sig);
    let returns_bool = returns_named(sig, "bool");
    let role = classify_role(ident, sig, kind, &peek, returns_self, returns_bool);
    let complexity = classify_complexity(body_lines, returns_result, &peek);
    FnContext {
        role,
        complexity,
        param_names: param_names(sig),
        returns_result,
        returns_self,
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
) -> FunctionRole {
    if is_constructor(ident, returns_self) {
        return FunctionRole::Constructor;
    }
    if is_getter(ident, sig, peek) {
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
    if ident.starts_with("render_") {
        return FunctionRole::Render;
    }
    if kind == FunctionKind::TraitImplMethod {
        return FunctionRole::TraitSurface;
    }
    if matches!(ident, "run" | "run_session" | "main") {
        return FunctionRole::Entry;
    }
    FunctionRole::Other
}

fn is_constructor(ident: &str, returns_self: bool) -> bool {
    matches!(ident, "new" | "try_new" | "default")
        || ((ident == "from" || ident.starts_with("from_")) && returns_self)
}

fn is_getter(ident: &str, sig: &Signature, peek: &BodyPeek) -> bool {
    if ident.starts_with("as_")
        || ident.starts_with("to_")
        || ident == "id"
        || ident.ends_with("_dir")
        || ident.ends_with("_path")
        || ident.ends_with("_name")
    {
        return true;
    }
    let Some(recv) = sig.receiver() else {
        return false;
    };
    recv.reference.is_some() && recv.mutability.is_none() && peek.stmts <= 2 && !peek.has_branch
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
    if returns_result || peek.has_try {
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

fn returns_named(sig: &Signature, name: &str) -> bool {
    match &sig.output {
        ReturnType::Type(_, ty) => type_path_contains(ty, name),
        ReturnType::Default => false,
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
    has_try: bool,
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
        has_try: false,
        has_branch: false,
        has_error_path_event: false,
    };
    BodyPeekVisitor(&mut peek).visit_block(block);
    peek
}

struct BodyPeekVisitor<'a>(&'a mut BodyPeek);

impl<'ast> Visit<'ast> for BodyPeekVisitor<'_> {
    fn visit_expr_try(&mut self, node: &'ast syn::ExprTry) {
        self.0.has_try = true;
        syn::visit::visit_expr_try(self, node);
    }

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
