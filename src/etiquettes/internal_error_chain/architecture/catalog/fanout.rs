//! Kind-variant fan-out facts: the wrapper and call records, and their extraction
//! from literal calls and one-hop string wrappers in canonical syntax.

use super::syntax;
use crate::loader::DatabaseMacroSyntax;
use ra_ap_syntax::ast::{self, HasArgList, HasGenericArgs, HasName};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    derive_new::new,
    derive_getters::Getters,
)]
pub(crate) struct FanoutTarget {
    kind: Option<String>,
    variant: Option<String>,
    source: String,
}

/// A wrapper constructor is matched by its owning type (`None` for a
/// free function) plus its own name -- never by the file/module it
/// happens to be defined in. These constructors (`AmenableError::io`,
/// `::invariant`, and similar) are deliberately defined once and
/// called from every other file in the crate; keying the match on the
/// definition site's file silently dropped every one of those calls.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub(crate) struct FanoutWrapper {
    file: PathBuf,
    self_ty: Option<String>,
    name: String,
    #[getter(copy)]
    line: u32,
    target: FanoutTarget,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_new::new, derive_getters::Getters,
)]
pub(crate) struct FanoutCall {
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    self_ty: Option<String>,
    callee: String,
    target: Option<FanoutTarget>,
    literal: String,
}

#[tracing::instrument(level = "trace", skip(function))]
pub(super) fn wrapper(function: &ast::Fn) -> Option<FanoutTarget> {
    function.ret_type()?;
    let params = function.param_list()?;
    if params.self_param().is_some() {
        return None;
    }
    let mut params = params.params();
    let param = params.next()?;
    if params.next().is_some() || !string_input(&param.ty()?) {
        return None;
    }
    let ast::Pat::IdentPat(param) = param.pat()? else {
        return None;
    };
    let name = param.name()?;
    let block = function.body()?.stmt_list()?;
    if block.statements().next().is_some() {
        return None;
    }
    forwarded(&block.tail_expr()?, name.text())
}
#[tracing::instrument(level = "trace", skip(ty))]
fn string_input(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::PathType(t) => t
            .path()
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .is_some_and(|n| n.text() == "String"),
        ast::Type::RefType(t) => {
            t.ty().and_then(|t| syntax::simple_type(&t)).as_deref() == Some("str")
        }
        ast::Type::ImplTraitType(t) => t
            .type_bound_list()
            .into_iter()
            .flat_map(|l| l.bounds())
            .any(|bound| {
                let Some(ast::Type::PathType(ty)) = bound.ty() else {
                    return false;
                };
                let Some(segment) = ty.path().and_then(|p| p.segment()) else {
                    return false;
                };
                segment.name_ref().is_some_and(|n| n.text() == "Into")
                    && segment.generic_arg_list().is_some_and(|args| {
                        args.generic_args().any(|arg| {
                            let ast::GenericArg::TypeArg(arg) = arg else {
                                return false;
                            };
                            arg.ty().and_then(|ty| syntax::simple_type(&ty)).as_deref()
                                == Some("String")
                        })
                    })
            }),
        _ => false,
    }
}
#[tracing::instrument(level = "trace", skip(expr))]
fn forwarded(expr: &ast::Expr, param: &str) -> Option<FanoutTarget> {
    let ast::Expr::CallExpr(call) = syntax::peel(expr.clone()) else {
        return None;
    };
    let arg = syntax::one_arg(&call)?;
    let (owner, name) = syntax::callee(&call.expr()?)?;
    if name == "new"
        && let Some(owner) = owner.as_ref().filter(|o| o.ends_with("Source"))
    {
        let ast::Expr::PathExpr(path) = syntax::peel(arg) else {
            return None;
        };
        return (path.path().and_then(|p| syntax::simple_path(&p)).as_deref() == Some(param))
            .then(|| FanoutTarget::new(None, None, owner.clone()));
    }
    let target = forwarded(&arg, param)?;
    if let Some(kind) = owner.filter(|o| o.ends_with("Kind")) {
        if target.kind().is_some() {
            return None;
        }
        return Some(FanoutTarget::new(
            Some(kind),
            Some(name),
            target.source().clone(),
        ));
    }
    Some(target)
}
#[tracing::instrument(level = "trace", skip(call, macros))]
pub(super) fn direct(
    call: &ast::CallExpr,
    macros: &DatabaseMacroSyntax,
    kind: bool,
) -> Option<(FanoutTarget, String)> {
    let arg = syntax::one_arg(call)?;
    let (owner, name) = syntax::callee(&call.expr()?)?;
    let owner = owner?;
    if !kind {
        return (name == "new" && owner.ends_with("Source"))
            .then(|| literal(&arg, macros).map(|l| (FanoutTarget::new(None, None, owner), l)))
            .flatten();
    }
    if !owner.ends_with("Kind") {
        return None;
    }
    let ast::Expr::CallExpr(source) = syntax::peel(arg) else {
        return None;
    };
    let (source_owner, source_name) = syntax::callee(&source.expr()?)?;
    let source_owner = source_owner?;
    if source_name != "new" || !source_owner.ends_with("Source") {
        return None;
    }
    Some((
        FanoutTarget::new(Some(owner), Some(name), source_owner),
        literal(&syntax::one_arg(&source)?, macros)?,
    ))
}
#[tracing::instrument(level = "trace", skip(expr, macros))]
pub(super) fn literal(expr: &ast::Expr, macros: &DatabaseMacroSyntax) -> Option<String> {
    match syntax::peel(expr.clone()) {
        ast::Expr::Literal(l) => string(&l),
        ast::Expr::MethodCallExpr(call)
            if call.arg_list()?.args().next().is_none()
                && call
                    .name_ref()
                    .is_some_and(|n| matches!(n.text(), "to_owned" | "into" | "to_string")) =>
        {
            literal(&call.receiver()?, macros)
        }
        ast::Expr::MacroExpr(m) => {
            let invocation = m.macro_call()?;
            if invocation
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .is_none_or(|n| n.text() != "format")
            {
                return None;
            }
            let ast::Expr::Literal(l) = macros.arguments(&invocation).first()? else {
                return None;
            };
            string(l)
        }
        _ => None,
    }
}
#[tracing::instrument(level = "trace", skip(literal))]
fn string(literal: &ast::Literal) -> Option<String> {
    let ast::LiteralKind::String(s) = literal.kind() else {
        return None;
    };
    s.value().ok().map(|s| s.into_owned())
}
