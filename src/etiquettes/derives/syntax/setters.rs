//! Setter body classification.

use syn::{Expr, FnArg, Pat, Signature, Stmt};
use tracing::instrument;

use super::exprs::{expr_is_self, expr_is_self_field, non_item_stmts, stmt_tail_expr};

/// Setter body that `derive_setters` can emit, including `into` and `strip_option`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::etiquettes::derives) enum SetterShape {
    Assign,
    Into,
    StripOption,
    StripOptionInto,
}

impl SetterShape {
    #[instrument(level = "trace", skip(self))]
    pub(in crate::etiquettes::derives) fn recommendation(self) -> &'static str {
        match self {
            Self::Assign => {
                "Use #[derive(derive_setters::Setters)] with #[setters(prefix = \"with_\")]"
            }
            Self::Into => {
                "Use #[derive(derive_setters::Setters)] with #[setters(prefix = \"with_\", into)]"
            }
            Self::StripOption => {
                "Use #[derive(derive_setters::Setters)] with #[setters(prefix = \"with_\", strip_option)]"
            }
            Self::StripOptionInto => {
                "Use #[derive(derive_setters::Setters)] with #[setters(prefix = \"with_\", strip_option, into)]"
            }
        }
    }
}

#[instrument(level = "debug", skip(block, sig), ret)]
pub(in crate::etiquettes::derives) fn classify_setter_body(
    block: &syn::Block,
    field_name: &str,
    sig: &Signature,
) -> Option<SetterShape> {
    let params = value_param_names(sig);
    if params.len() != 1 {
        return None;
    }
    let stmts = non_item_stmts(block);
    let assign = match stmts.as_slice() {
        [assign] => *assign,
        [assign, ret] if stmt_is_return_self(ret) => *assign,
        _ => return None,
    };
    stmt_setter_shape(assign, field_name, &params)
}

#[instrument(level = "debug", skip(sig))]
fn value_param_names(sig: &Signature) -> Vec<String> {
    sig.inputs
        .iter()
        .filter_map(|arg| {
            let FnArg::Typed(pat_type) = arg else {
                return None;
            };
            let Pat::Ident(ident) = &*pat_type.pat else {
                return None;
            };
            Some(ident.ident.to_string())
        })
        .collect()
}

#[instrument(level = "debug", skip(stmt, params), ret)]
fn stmt_setter_shape(stmt: &Stmt, field_name: &str, params: &[String]) -> Option<SetterShape> {
    let Expr::Assign(assign) = stmt_tail_expr(stmt)? else {
        return None;
    };
    if !expr_is_self_field(&assign.left, field_name) {
        return None;
    }
    classify_setter_rhs(&assign.right, params)
}

#[instrument(level = "debug", skip(stmt), ret)]
fn stmt_is_return_self(stmt: &Stmt) -> bool {
    let Some(expr) = stmt_tail_expr(stmt) else {
        return false;
    };
    match expr {
        Expr::Return(return_expr) => return_expr
            .expr
            .as_ref()
            .is_some_and(|inner| expr_is_self(inner)),
        other => expr_is_self(other),
    }
}

#[instrument(level = "debug", skip(expr, params), ret)]
fn classify_setter_rhs(expr: &Expr, params: &[String]) -> Option<SetterShape> {
    match expr {
        Expr::Call(call) if expr_is_some_ctor(&call.func) && call.args.len() == 1 => {
            match classify_owned_input(&call.args[0], params)? {
                OwnedInput::Direct => Some(SetterShape::StripOption),
                OwnedInput::Into => Some(SetterShape::StripOptionInto),
            }
        }
        Expr::Paren(paren) => classify_setter_rhs(&paren.expr, params),
        Expr::Group(group) => classify_setter_rhs(&group.expr, params),
        other => match classify_owned_input(other, params)? {
            OwnedInput::Direct => Some(SetterShape::Assign),
            OwnedInput::Into => Some(SetterShape::Into),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnedInput {
    Direct,
    Into,
}

#[instrument(level = "debug", skip(expr, params), ret)]
fn classify_owned_input(expr: &Expr, params: &[String]) -> Option<OwnedInput> {
    match expr {
        Expr::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|ident| params.iter().any(|param| ident == param))
            .then_some(OwnedInput::Direct),
        Expr::MethodCall(call)
            if call.args.is_empty()
                && matches!(
                    call.method.to_string().as_str(),
                    "into" | "clone" | "to_owned" | "to_string"
                ) =>
        {
            classify_owned_input(&call.receiver, params).map(|_| OwnedInput::Into)
        }
        Expr::Paren(paren) => classify_owned_input(&paren.expr, params),
        Expr::Group(group) => classify_owned_input(&group.expr, params),
        _ => None,
    }
}

#[instrument(level = "debug", skip(expr), ret)]
fn expr_is_some_ctor(expr: &Expr) -> bool {
    let Expr::Path(path) = expr else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Some")
}
