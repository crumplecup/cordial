//! Recognize written contracts without confusing them with semantic identity.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs, HasName},
};

use super::{SourceEntryContract, SourceResultContract};
use crate::loader::DatabaseSource;

#[tracing::instrument(level = "trace", skip(function))]
pub(in crate::ir::function_source) fn entry_contract(
    function: &ast::Fn,
) -> Option<SourceEntryContract> {
    let parameters = function.param_list()?;
    let unit = function.ret_type().is_none()
        || matches!(
            function.ret_type().and_then(|output| output.ty()),
            Some(ast::Type::TupleType(tuple)) if tuple.fields().next().is_none()
        );
    (function.name()?.text() == "main"
        && parameters.params().next().is_none()
        && parameters.self_param().is_none()
        && unit)
        .then_some(SourceEntryContract::UnitMain)
}

#[tracing::instrument(level = "trace", skip(file, output))]
pub(in crate::ir::function_source) fn result_contract(
    file: &DatabaseSource<'_>,
    output: Option<&ast::Type>,
) -> Option<SourceResultContract> {
    let ast::Type::PathType(path_type) = output? else {
        return None;
    };
    let path = path_type.path()?;
    let segments = segments(&path)?;
    if segments.len() != 3
        || !matches!(segments[0].as_str(), "std" | "core")
        || segments[1] != "result"
        || segments[2] != "Result"
    {
        return None;
    }
    let mut arguments = path.segment()?.generic_arg_list()?.generic_args();
    let ast::GenericArg::TypeArg(success) = arguments.next()? else {
        return None;
    };
    let ast::GenericArg::TypeArg(error) = arguments.next()? else {
        return None;
    };
    if arguments.next().is_some() {
        return None;
    }
    let success = success.ty()?;
    let error = error.ty()?;
    Some(SourceResultContract {
        success_syntax: file.text(success.syntax()).to_owned(),
        error_syntax: file.text(error.syntax()).to_owned(),
        error_identity: standard_type_identity(&error),
    })
}

#[tracing::instrument(level = "trace", skip(ty))]
fn standard_type_identity(ty: &ast::Type) -> Option<String> {
    let ast::Type::PathType(path_type) = ty else {
        return None;
    };
    let path = path_type.path()?;
    if path.segment()?.generic_arg_list().is_some() {
        return None;
    }
    let segments = segments(&path)?;
    if !matches!(segments.first()?.as_str(), "std" | "core") {
        return None;
    }
    Some(format!("::{}", segments.join("::")))
}

#[tracing::instrument(level = "trace", skip(output))]
pub(in crate::ir::function_source) fn has_infallible_result_syntax(
    output: Option<&ast::Type>,
) -> bool {
    let Some(ast::Type::PathType(path_type)) = output else {
        return false;
    };
    let Some(path) = path_type.path() else {
        return false;
    };
    segments(&path).is_some_and(|names| names == ["core", "result", "Result"])
}

/// Reject anchors and generic qualifiers; only the final segment may have args.
#[tracing::instrument(level = "trace", skip(path))]
fn segments(path: &ast::Path) -> Option<Vec<String>> {
    let mut names = Vec::new();
    let mut current = path.clone();
    loop {
        let segment = current.segment()?;
        if segment.type_anchor().is_some()
            || (!names.is_empty() && segment.generic_arg_list().is_some())
        {
            return None;
        }
        names.push(segment.name_ref()?.text().to_string());
        if let Some(qualifier) = current.qualifier() {
            current = qualifier;
        } else {
            segment.coloncolon_token()?;
            break;
        }
    }
    names.reverse();
    Some(names)
}
