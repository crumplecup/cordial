//! Token normalization and top-level token-shape utilities.

use super::super::token_tree::{Delimiter, Token, parse, render};
use ra_ap_syntax::{AstNode, Edition, ast};
use tracing::instrument;

/// Drop the `::` a call-site turbofish (`Type::<Args>`) writes before its
/// generic argument list.
#[instrument(level = "debug")]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn strip_turbofish(text: &str) -> String {
    text.replace(" :: <", " <")
}

/// Strip every top-level `#[trigger]` attribute from `tokens`.
#[instrument(level = "debug", skip(tokens))]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn strip_trigger_attrs(
    tokens: Vec<Token>,
) -> Vec<Token> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut items = tokens.into_iter().peekable();
    while let Some(token) = items.next() {
        if token.is_punct('#')
            && items.peek().is_some_and(|next| {
                next.group(Delimiter::Bracket)
                    .is_some_and(|inner| render(inner) == "trigger")
            })
        {
            items.next();
            continue;
        }
        out.push(token);
    }
    out
}

/// Recognize a whole-clause bare call `name(...)` or `!name(...)`
/// directly from tokens without requiring the argument list to parse as
/// plain Rust syntax.
#[instrument(level = "debug", skip(clause))]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn bare_named_call_name(
    clause: &[Token],
) -> Option<String> {
    let items = match clause {
        [bang, rest @ ..] if bang.is_punct('!') => rest,
        rest => rest,
    };
    match items {
        [name, arguments] if arguments.group(Delimiter::Parenthesis).is_some() => {
            name.ident().map(str::to_string)
        }
        _ => None,
    }
}

/// The clause as a Rust expression, when its tokens form exactly one.
#[instrument(level = "debug", skip(clause))]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn expression(
    clause: &[Token],
) -> Option<ast::Expr> {
    let parse = ast::Expr::parse(&render(clause), Edition::CURRENT);
    if !parse.errors().is_empty() {
        return None;
    }
    ast::Expr::cast(parse.syntax_node())
}

/// [`bare_named_call_name`], extended to also recognize a Creusot/Verus
/// bare call carrying a leading outer attribute (`#[trigger] name(...)`).
#[instrument(level = "debug", skip(clause))]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn named_call_name_allowing_leading_attr(
    clause: &[Token],
) -> Option<String> {
    if let Some(name) = bare_named_call_name(clause) {
        return Some(name);
    }
    match super::super::facts::ContractCall::from_expr(&expression(clause)?) {
        super::super::facts::ContractCall::Bare(name) => Some(name),
        _ => None,
    }
}

/// Re-tokenize and re-stringify a fragment or clause into a canonical
/// form for the type-prefix suffix comparison.
#[instrument(level = "debug")]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn normalize_text(text: &str) -> String {
    parse(text).map_or_else(
        || canonicalize_type_text(text.trim()),
        |tokens| canonicalize_type_text(&render(&tokens)),
    )
}

/// Canonicalize `text` for type-prefix suffix comparison by stripping
/// whitespace and collapsing an elidable trailing comma before `>`.
#[instrument(level = "debug")]
pub(in crate::etiquettes::antipatterns::contract_bounds) fn canonicalize_type_text(
    text: &str,
) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace(",>", ">")
}
