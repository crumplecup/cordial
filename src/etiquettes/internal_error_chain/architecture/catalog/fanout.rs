//! Syntactic facts for one-hop Kind-variant fan-out detection.

use std::path::PathBuf;

use syn::{
    Expr, FnArg, GenericArgument, Lit, Pat, PathArguments, ReturnType, Signature, Stmt, Type,
};
use tracing::instrument;

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, derive_new::new, derive_getters::Getters,
)]
pub(crate) struct FanoutTarget {
    kind: Option<String>,
    variant: Option<String>,
    source: String,
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub(crate) struct FanoutWrapper {
    file: PathBuf,
    module: Vec<String>,
    name: String,
    #[getter(copy)]
    line: u32,
    target: FanoutTarget,
}

#[derive(Debug, Clone, derive_new::new, derive_getters::Getters)]
pub(crate) struct FanoutCall {
    file: PathBuf,
    module: Vec<String>,
    #[getter(copy)]
    line: u32,
    callee: String,
    target: Option<FanoutTarget>,
    literal: String,
}

#[instrument(level = "trace", skip(sig, block))]
pub(super) fn wrapper_target(sig: &Signature, block: &syn::Block) -> Option<FanoutTarget> {
    if matches!(sig.output, ReturnType::Default) || sig.inputs.len() != 1 || block.stmts.len() != 1
    {
        return None;
    }
    let FnArg::Typed(input) = sig.inputs.first()? else {
        return None;
    };
    if !is_string_input(&input.ty) {
        return None;
    }
    let Pat::Ident(param) = input.pat.as_ref() else {
        return None;
    };
    let Stmt::Expr(expr, None) = block.stmts.first()? else {
        return None;
    };
    forwarded_target(expr, &param.ident.to_string())
}

#[instrument(level = "trace", skip(ty))]
fn is_string_input(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path.path.segments.last().is_some_and(|seg| seg.ident == "String"),
        Type::Reference(reference) => matches!(reference.elem.as_ref(), Type::Path(path) if path.path.is_ident("str")),
        Type::ImplTrait(bounds) => bounds.bounds.iter().any(|bound| {
            let syn::TypeParamBound::Trait(bound) = bound else { return false };
            let Some(seg) = bound.path.segments.last() else { return false };
            if seg.ident != "Into" { return false; }
            let PathArguments::AngleBracketed(args) = &seg.arguments else { return false; };
            args.args.iter().any(|arg| matches!(arg, GenericArgument::Type(Type::Path(path)) if path.path.is_ident("String")))
        }),
        _ => false,
    }
}

#[instrument(level = "trace", skip(expr))]
fn forwarded_target(expr: &Expr, param: &str) -> Option<FanoutTarget> {
    let Expr::Call(call) = peel(expr) else {
        return None;
    };
    if call.args.len() != 1 {
        return None;
    }
    let path = call_path(&call.func)?;
    let pair = last_two(path);
    let arg = call.args.first()?;
    if let Some((name, owner)) = pair
        && name == "new"
        && owner.to_string().ends_with("Source")
    {
        return path_is_ident(arg, param).then(|| FanoutTarget::new(None, None, owner.to_string()));
    }
    let target = forwarded_target(arg, param)?;
    if let Some((variant, kind)) = pair
        && kind.to_string().ends_with("Kind")
    {
        if target.kind().is_some() {
            return None;
        }
        return Some(FanoutTarget::new(
            Some(kind.to_string()),
            Some(variant.to_string()),
            target.source().clone(),
        ));
    }
    Some(target)
}

#[instrument(level = "trace", skip(call))]
pub(super) fn direct_target(call: &syn::ExprCall) -> Option<(FanoutTarget, String)> {
    if call.args.len() != 1 {
        return None;
    }
    let path = call_path(&call.func)?;
    let (variant, kind) = last_two(path)?;
    if !kind.to_string().ends_with("Kind") {
        return None;
    }
    let Expr::Call(source_call) = peel(call.args.first()?) else {
        return None;
    };
    if source_call.args.len() != 1 {
        return None;
    }
    let source_path = call_path(&source_call.func)?;
    let (name, source) = last_two(source_path)?;
    if name != "new" || !source.to_string().ends_with("Source") {
        return None;
    }
    let literal = literal_string(source_call.args.first()?)?;
    Some((
        FanoutTarget::new(
            Some(kind.to_string()),
            Some(variant.to_string()),
            source.to_string(),
        ),
        literal,
    ))
}

#[instrument(level = "trace", skip(call))]
pub(super) fn direct_source_target(call: &syn::ExprCall) -> Option<(FanoutTarget, String)> {
    if call.args.len() != 1 {
        return None;
    }
    let path = call_path(&call.func)?;
    let (name, source) = last_two(path)?;
    if name != "new" || !source.to_string().ends_with("Source") {
        return None;
    }
    let literal = literal_string(call.args.first()?)?;
    Some((FanoutTarget::new(None, None, source.to_string()), literal))
}

#[instrument(level = "trace", skip(expr))]
pub(super) fn literal_string(expr: &Expr) -> Option<String> {
    match peel(expr) {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Str(value) => Some(value.value()),
            _ => None,
        },
        Expr::MethodCall(call)
            if call.args.is_empty()
                && (call.method == "to_owned"
                    || call.method == "into"
                    || call.method == "to_string") =>
        {
            literal_string(&call.receiver)
        }
        _ => None,
    }
}

#[instrument(level = "trace", skip(expr))]
pub(super) fn call_name(expr: &Expr) -> Option<String> {
    let path = call_path(expr)?;
    (path.segments.len() == 1).then(|| path.segments[0].ident.to_string())
}

#[instrument(level = "trace", skip(expr))]
fn call_path(expr: &Expr) -> Option<&syn::Path> {
    let Expr::Path(path) = peel(expr) else {
        return None;
    };
    Some(&path.path)
}

#[instrument(level = "trace", skip(path))]
fn last_two(path: &syn::Path) -> Option<(&syn::Ident, &syn::Ident)> {
    let mut segments = path.segments.iter().rev();
    Some((&segments.next()?.ident, &segments.next()?.ident))
}

#[instrument(level = "trace", skip(expr))]
fn path_is_ident(expr: &Expr, ident: &str) -> bool {
    matches!(peel(expr), Expr::Path(path) if path.path.is_ident(ident))
}

#[instrument(level = "trace", skip(expr))]
fn peel(mut expr: &Expr) -> &Expr {
    loop {
        expr = match expr {
            Expr::Paren(paren) => &paren.expr,
            Expr::Group(group) => &group.expr,
            _ => return expr,
        };
    }
}
