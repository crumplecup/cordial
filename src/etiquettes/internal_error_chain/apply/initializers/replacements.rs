//! Proven expression rewrites for error field initializers.

use std::collections::{BTreeMap, BTreeSet};

use syn::Expr;
use syn::spanned::Spanned;
use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::super::span_range;

#[instrument(level = "debug", skip(source, expr, strategy, aliases))]
pub(super) fn file_initializer_replacement(
    source: &str,
    expr: &Expr,
    strategy: ErrorFieldStrategy,
    aliases: &BTreeSet<String>,
) -> Option<String> {
    let expr = without_parens(expr);
    let file_call = match strategy {
        ErrorFieldStrategy::Borrowed => match expr {
            Expr::MethodCall(method)
                if matches!(
                    method.method.to_string().as_str(),
                    "to_string" | "to_owned" | "into"
                ) && method.args.is_empty() =>
            {
                &*method.receiver
            }
            _ => string_from_argument(expr)?,
        },
        ErrorFieldStrategy::Owned => expr,
    };
    let file_call = without_parens(file_call);
    let Expr::MethodCall(file_method) = file_call else {
        return None;
    };
    if file_method.method != "file" || !file_method.args.is_empty() {
        return None;
    }
    let valid_receiver = match without_parens(&file_method.receiver) {
        Expr::Path(path) => path
            .path
            .get_ident()
            .is_some_and(|name| aliases.contains(&name.to_string())),
        Expr::Call(call) => is_location_caller(call),
        _ => false,
    };
    if !valid_receiver {
        return None;
    }
    let (start, end) = span_range(source, file_call.span())?;
    let file_text = source.get(start..end)?;
    Some(match strategy {
        ErrorFieldStrategy::Borrowed => file_text.to_string(),
        ErrorFieldStrategy::Owned => format!("{file_text}.to_string()"),
    })
}

#[instrument(
    level = "debug",
    skip(source, expr, strategy, static_strings, static_params, static_locals)
)]
pub(super) fn static_string_initializer_replacement(
    source: &str,
    expr: &Expr,
    strategy: ErrorFieldStrategy,
    static_strings: &BTreeSet<String>,
    static_params: &BTreeSet<String>,
    static_locals: &BTreeMap<String, usize>,
) -> Option<String> {
    let expr = without_parens(expr);
    let value = match strategy {
        ErrorFieldStrategy::Borrowed => match expr {
            Expr::MethodCall(method)
                if matches!(
                    method.method.to_string().as_str(),
                    "to_string" | "to_owned" | "into"
                ) && method.args.is_empty() =>
            {
                &*method.receiver
            }
            Expr::Call(_) => string_from_argument(expr)?,
            _ => return None,
        },
        ErrorFieldStrategy::Owned => expr,
    };
    let value = without_parens(value);
    let (start, end) = span_range(source, value.span())?;
    let local_static_source = matches!(value, Expr::Path(path)
    if path.qself.is_none()
        && path.path.get_ident().is_some_and(|name| {
            let name = name.to_string();
            static_params.contains(&name)
                || static_locals.get(&name).is_some_and(|declaration_end| *declaration_end <= start)
        }));
    let literal = matches!(value, Expr::Lit(lit) if matches!(&lit.lit, syn::Lit::Str(_)));
    let qualified_const = matches!(value, Expr::Path(path)
        if path.qself.is_none()
            && matches!(path.path.segments.iter().map(|part| part.ident.to_string()).collect::<Vec<_>>().as_slice(), [scope, name]
                if scope == "self" && static_strings.contains(name)));
    let qualified_builtin_macro = matches!(value, Expr::Macro(macro_expr)
        if macro_expr.mac.path.leading_colon.is_some()
            && matches!(macro_expr.mac.path.segments.iter().map(|part| part.ident.to_string()).collect::<Vec<_>>().as_slice(), [root, name]
                if (root == "core" || root == "std") && (name == "concat" || name == "include_str")));
    if !literal && !qualified_const && !qualified_builtin_macro && !local_static_source {
        return None;
    }
    let value_text = source.get(start..end)?;
    Some(match strategy {
        ErrorFieldStrategy::Borrowed => value_text.to_string(),
        ErrorFieldStrategy::Owned if local_static_source => format!("String::from({value_text})"),
        ErrorFieldStrategy::Owned => format!("{value_text}.to_string()"),
    })
}

#[instrument(level = "debug", skip(expr))]
fn string_from_argument(expr: &Expr) -> Option<&Expr> {
    let Expr::Call(call) = without_parens(expr) else {
        return None;
    };
    if call.args.len() != 1 {
        return None;
    }
    let Expr::Path(path) = without_parens(&call.func) else {
        return None;
    };
    if path.qself.is_some() {
        return None;
    }
    let names: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    if !matches!(names.as_slice(), [string, from] if string == "String" && from == "from")
        && !matches!(names.as_slice(), [root, module, string, from]
            if (root == "std" || root == "alloc") && module == "string" && string == "String" && from == "from")
    {
        return None;
    }
    call.args.first()
}

#[instrument(level = "debug", skip(expr))]
fn without_parens(mut expr: &Expr) -> &Expr {
    while let Expr::Paren(paren) = expr {
        expr = &paren.expr;
    }
    expr
}

#[instrument(level = "trace", skip(call))]
pub(super) fn is_location_caller(call: &syn::ExprCall) -> bool {
    if !call.args.is_empty() {
        return false;
    }
    let Expr::Path(path) = &*call.func else {
        return false;
    };
    let names: Vec<String> = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    matches!(names.as_slice(), [location, caller] if location == "Location" && caller == "caller")
        || matches!(names.as_slice(), [root, panic, location, caller]
            if (root == "std" || root == "core") && panic == "panic" && location == "Location" && caller == "caller")
}
