//! Located instrument arguments projected from the canonical attribute tokens.

use super::super::SourceByteRange;
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// One instrument annotation, including its qualification and enclosing gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceInstrumentSyntax {
    meta_range: SourceByteRange,
    gate_predicate: Option<String>,
    arguments: Option<SourceAttributeArguments>,
}

/// A parenthesized argument list with delimiters excluded from its range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceAttributeArguments {
    range: SourceByteRange,
    entries: Vec<SourceAttributeArgument>,
    #[getter(copy)]
    trailing_comma: bool,
}

/// A named argument and any value or nested list, without interpreting expressions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct SourceAttributeArgument {
    name: String,
    range: SourceByteRange,
    value_range: Option<SourceByteRange>,
    arguments: Option<SourceAttributeArguments>,
}

pub(super) struct LocatedToken {
    text: String,
    range: std::ops::Range<usize>,
    group: Option<LocatedGroup>,
}

pub(super) struct LocatedGroup {
    range: std::ops::Range<usize>,
    tokens: Vec<LocatedToken>,
}

/// Tokens come from a parsed attribute; this projects their locations only.
#[instrument(level = "debug", skip(tokens))]
pub(super) fn collect(
    tokens: &[LocatedToken],
    gate_predicate: Option<String>,
) -> Option<SourceInstrumentSyntax> {
    let first = tokens.first()?;
    let group_at = tokens.iter().position(|token| token.group.is_some());
    let path_end = group_at.unwrap_or(tokens.len());
    let path: String = tokens[..path_end]
        .iter()
        .map(|token| token.text.as_str())
        .collect();
    if path == "cfg_attr" {
        if gate_predicate.is_some() {
            return None;
        }
        let group = tokens.get(path_end)?.group.as_ref()?;
        let comma = group.tokens.iter().position(|token| token.text == ",")?;
        return collect(
            &group.tokens[comma + 1..],
            Some(compact_tokens(&group.tokens[..comma])),
        );
    }
    if !matches!(
        path.trim_start_matches("::"),
        "instrument" | "tracing::instrument"
    ) {
        return None;
    }
    let arguments = match group_at {
        Some(index) if index + 1 == tokens.len() => Some(arguments(tokens[index].group.as_ref()?)),
        Some(_) => return None,
        None => None,
    };
    Some(SourceInstrumentSyntax {
        meta_range: (first.range.start..tokens.last()?.range.end).into(),
        gate_predicate,
        arguments,
    })
}

#[instrument(level = "trace", skip(tokens))]
fn compact_tokens(tokens: &[LocatedToken]) -> String {
    tokens
        .iter()
        .map(|token| match &token.group {
            Some(group) => format!("({})", compact_tokens(&group.tokens)),
            None => token.text.clone(),
        })
        .collect()
}

#[instrument(level = "debug", skip(group))]
fn arguments(group: &LocatedGroup) -> SourceAttributeArguments {
    let entries = group
        .tokens
        .split(|token| token.text == ",")
        .filter_map(|tokens| {
            let first = tokens.first()?;
            let last = tokens.last()?;
            Some(SourceAttributeArgument {
                name: tokens
                    .iter()
                    .take_while(|token| token.text != "=" && token.group.is_none())
                    .map(|token| token.text.as_str())
                    .collect(),
                range: (first.range.start..last.range.end).into(),
                value_range: tokens
                    .get(1)
                    .filter(|token| token.text == "=")
                    .and_then(|_| tokens.get(2))
                    .map(|value| (value.range.start..last.range.end).into()),
                arguments: tokens
                    .get(1)
                    .and_then(|token| token.group.as_ref())
                    .map(arguments),
            })
        })
        .collect();
    SourceAttributeArguments {
        range: group.range.clone().into(),
        entries,
        trailing_comma: group.tokens.last().is_some_and(|token| token.text == ","),
    }
}

#[instrument(level = "trace", skip(tokens))]
pub(super) fn compatibility_tokens(tokens: proc_macro2::TokenStream) -> Vec<LocatedToken> {
    tokens
        .into_iter()
        .map(|token| {
            let range = token.span().byte_range();
            match token {
                proc_macro2::TokenTree::Group(group) => LocatedToken {
                    text: String::new(),
                    range,
                    group: Some(LocatedGroup {
                        range: group.span_open().byte_range().end
                            ..group.span_close().byte_range().start,
                        tokens: compatibility_tokens(group.stream()),
                    }),
                },
                token => LocatedToken {
                    text: token.to_string(),
                    range,
                    group: None,
                },
            }
        })
        .collect()
}

#[instrument(level = "trace", skip(node))]
pub(super) fn native_tokens(node: &ra_ap_syntax::SyntaxNode) -> Vec<LocatedToken> {
    use ra_ap_syntax::{AstNode, ast};
    node.children_with_tokens()
        .flat_map(|element| {
            if let Some(node) = element.as_node() {
                if let Some(group) = ast::TokenTree::cast(node.clone()) {
                    let range = node.text_range();
                    let start = usize::from(range.start());
                    let end = usize::from(range.end());
                    // Token trees retain their parsed delimiters, including nested groups.
                    let mut tokens = native_tokens(group.syntax());
                    if tokens.len() >= 2 {
                        tokens.remove(0);
                        tokens.pop();
                    }
                    return vec![LocatedToken {
                        text: String::new(),
                        range: start..end,
                        group: Some(LocatedGroup {
                            range: start + 1..end.saturating_sub(1),
                            tokens,
                        }),
                    }];
                }
                return native_tokens(node);
            }
            let Some(token) = element
                .into_token()
                .filter(|token| !token.kind().is_trivia())
            else {
                return Vec::new();
            };
            let range = token.text_range();
            vec![LocatedToken {
                text: token.text().to_string(),
                range: usize::from(range.start())..usize::from(range.end()),
                group: None,
            }]
        })
        .collect()
}
