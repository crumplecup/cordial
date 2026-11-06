//! Closed forwarding functions and their located return type edits.
use super::native_methods as methods;
use super::native_syntax::{
    one_lifetime, position, range, scalar, single_segment, type_with_lifetime,
};
use crate::config::ErrorFieldStrategy;
use crate::etiquettes::internal_error_chain::apply::lifetime::ReturnPlanFacts;
use crate::loader::DatabaseSource;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericArgs, HasName, HasVisibility},
};
use tracing::instrument;

#[instrument(level = "trace", skip(file, function, strategy))]
pub(super) fn forwarding_return(
    file: &DatabaseSource<'_>,
    function: &ast::Fn,
    name: &str,
    field: &str,
    constructor: &str,
    lifetime: &str,
    strategy: ErrorFieldStrategy,
) -> Option<ReturnPlanFacts> {
    if !methods::plain_function(function)
        || function.visibility().is_some()
        || one_lifetime(function).as_deref() != Some(lifetime)
    {
        return None;
    }
    let list = function.param_list()?;
    if list.self_param().is_some() {
        return None;
    }
    let inputs: Vec<_> = list.params().collect();
    if !methods::first_parameter(inputs.first()?, field, lifetime) {
        return None;
    }
    let return_ty = function.ret_type()?.ty()?;
    let correct = match strategy {
        ErrorFieldStrategy::Borrowed => bare_return(&return_ty, name),
        ErrorFieldStrategy::Owned => type_with_lifetime(&return_ty, name, lifetime),
    };
    if !correct {
        return None;
    }
    let block = function.body()?.stmt_list()?;
    if block.statements().next().is_some() {
        return None;
    }
    let ast::Expr::CallExpr(call) = block.tail_expr()? else {
        return None;
    };
    let ast::Expr::PathExpr(callee) = call.expr()? else {
        return None;
    };
    let path = callee.path()?;
    let method = path.segment()?;
    let owner = path.qualifier()?;
    let ty = owner.segment()?;
    if owner.qualifier().is_some() || !plain_part(&ty, name) || !plain_part(&method, constructor) {
        return None;
    }
    let args: Vec<_> = call.arg_list()?.args().collect();
    if args.len() != inputs.len() || !is_name(args.first()?, field) {
        return None;
    }
    if !inputs
        .iter()
        .skip(1)
        .zip(args.iter().skip(1))
        .all(|(input, arg)| {
            methods::parameter_pattern(input).is_some_and(|pattern| {
                pattern.ref_token().is_none()
                    && pattern.mut_token().is_none()
                    && pattern.name().is_some_and(|name| is_name(arg, name.text()))
            }) && input.ty().and_then(scalar).is_some()
        })
    {
        return None;
    }
    let range = range(return_ty.syntax());
    Some(ReturnPlanFacts {
        position: position(file.source(), range.0),
        range,
    })
}

#[instrument(level = "trace", skip(ty))]
fn bare_return(ty: &ast::Type, name: &str) -> bool {
    let Some(part) = single_segment(ty) else {
        return false;
    };
    plain_part(&part, name)
}

#[instrument(level = "trace", skip(part))]
fn plain_part(part: &ast::PathSegment, name: &str) -> bool {
    part.type_anchor().is_none()
        && part.generic_arg_list().is_none()
        && part.parenthesized_arg_list().is_none()
        && part.name_ref().is_some_and(|value| value.text() == name)
}

#[instrument(level = "trace", skip(expr))]
fn is_name(expr: &ast::Expr, name: &str) -> bool {
    matches!(expr, ast::Expr::PathExpr(expr) if expr.path().is_some_and(|path| {
        path.qualifier().is_none() && path.coloncolon_token().is_none()
            && path.segment().is_some_and(|part| part.coloncolon_token().is_none() && plain_part(&part, name))
    }))
}
