//! Typed initializer shapes collected from the shared tree and planned later.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use syn::Expr;
use syn::spanned::Spanned;
use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::super::{Edit, span_range};
use super::FieldKind;

type Range = (usize, usize);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum StorageForm {
    Bare,
    Method,
    StringFrom,
    EmptyString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum ValueSource {
    CallSiteFile,
    StaticString { local_binding: bool },
    EmptyFile,
}

/// One expression and the provenance needed to rewrite either storage form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct InitializerExpressionFact {
    whole: Range,
    value: Range,
    form: StorageForm,
    source: ValueSource,
    field_name: String,
    shorthand: bool,
}

impl InitializerExpressionFact {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn end(&self) -> usize {
        self.whole.1
    }

    #[instrument(level = "trace", skip(self, source, strategy))]
    pub(super) fn edit(&self, source: &str, strategy: ErrorFieldStrategy) -> Option<Edit> {
        match strategy {
            ErrorFieldStrategy::Borrowed if self.form == StorageForm::Bare => return None,
            ErrorFieldStrategy::Owned if self.form != StorageForm::Bare => return None,
            _ => {}
        }
        let value = source.get(self.value.0..self.value.1)?;
        let replacement = match (strategy, &self.source) {
            (ErrorFieldStrategy::Borrowed, ValueSource::EmptyFile) => "\"\"".to_string(),
            (ErrorFieldStrategy::Owned, ValueSource::EmptyFile) => "String::new()".to_string(),
            (ErrorFieldStrategy::Borrowed, _) => value.to_string(),
            (ErrorFieldStrategy::Owned, _) => match self.source {
                ValueSource::StaticString {
                    local_binding: true,
                } => format!("String::from({value})"),
                _ => format!("{value}.to_string()"),
            },
        };
        Some(Edit {
            start: self.whole.0,
            end: self.whole.1,
            replacement: if self.shorthand {
                format!("{}: {replacement}", self.field_name)
            } else {
                replacement
            },
        })
    }
}

/// The zero-location branch owns no runtime data and has an exact reverse form.
#[instrument(level = "trace", skip(source, expr))]
pub(super) fn inspect_empty_file(source: &str, expr: &Expr) -> Option<InitializerExpressionFact> {
    let form = match expr {
        Expr::Lit(lit) if matches!(&lit.lit, syn::Lit::Str(value) if value.value().is_empty()) => {
            StorageForm::Bare
        }
        Expr::Call(call)
            if call.args.is_empty()
                && matches!(&*call.func, Expr::Path(path) if path.qself.is_none()
                && path.path.segments.iter().all(|part| part.arguments.is_empty())
                && path.path.segments.iter().map(|part| part.ident.to_string()).eq(["String", "new"])) =>
        {
            StorageForm::EmptyString
        }
        _ => return None,
    };
    let whole = span_range(source, expr.span())?;
    Some(InitializerExpressionFact {
        whole,
        value: whole,
        form,
        source: ValueSource::EmptyFile,
        field_name: "file".to_string(),
        shorthand: false,
    })
}

#[instrument(
    level = "debug",
    skip(
        source,
        expr,
        kind,
        aliases,
        static_strings,
        static_params,
        static_locals
    )
)]
pub(super) fn inspect_initializer(
    source: &str,
    expr: &Expr,
    field_name: &str,
    shorthand: bool,
    kind: FieldKind,
    aliases: &BTreeSet<String>,
    static_strings: &BTreeSet<String>,
    static_params: &BTreeSet<String>,
    static_locals: &BTreeMap<String, usize>,
) -> Option<InitializerExpressionFact> {
    let whole = span_range(source, expr.span())?;
    let expr = without_parens(expr);
    let (form, value) = match expr {
        Expr::MethodCall(method)
            if matches!(
                method.method.to_string().as_str(),
                "to_string" | "to_owned" | "into"
            ) && method.args.is_empty() =>
        {
            (StorageForm::Method, &*method.receiver)
        }
        _ if string_from_argument(expr).is_some() => {
            (StorageForm::StringFrom, string_from_argument(expr)?)
        }
        _ => (StorageForm::Bare, expr),
    };
    let value = without_parens(value);
    let value_range = span_range(source, value.span())?;
    let source_kind = match kind {
        FieldKind::CallSiteFile => {
            let Expr::MethodCall(file_method) = value else {
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
            valid_receiver.then_some(ValueSource::CallSiteFile)?
        }
        FieldKind::StaticString => {
            let local_binding = matches!(value, Expr::Path(path)
            if path.qself.is_none()
                && path.path.get_ident().is_some_and(|name| {
                    let name = name.to_string();
                    static_params.contains(&name)
                        || static_locals.get(&name).is_some_and(|end| *end <= value_range.0)
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
            (local_binding || literal || qualified_const || qualified_builtin_macro)
                .then_some(ValueSource::StaticString { local_binding })?
        }
    };
    Some(InitializerExpressionFact {
        whole,
        value: value_range,
        form,
        source: source_kind,
        field_name: field_name.to_string(),
        shorthand,
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
