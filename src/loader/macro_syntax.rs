//! Shared syntax for explicit macro grammars; unknown payloads stay opaque.

use std::collections::BTreeMap;

use proc_macro2::{Delimiter, TokenTree};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::Visit;
use tracing::instrument;

/// Parsed Rust payload, owned by the same shared snapshot as its invocation.
/// Grammar recognition does not establish a macro's expansion semantics.
#[derive(Debug)]
pub enum SharedMacroSyntax {
    /// Expression arguments of standard assertion, formatting, or list macros.
    Expressions(Vec<syn::Expr>),
    /// Items in an Amenable harness's third, braced argument.
    Items(Vec<syn::Item>),
    /// Rust items in another macro's last braced argument. No expansion claim.
    TrailingItems(Vec<syn::Item>),
}

pub(super) type MacroTrees = BTreeMap<(usize, usize, usize, usize), SharedMacroSyntax>;

#[instrument(level = "trace", skip(invocation))]
pub(super) fn location(invocation: &syn::Macro) -> (usize, usize, usize, usize) {
    let span = invocation.span();
    (
        span.start().line,
        span.start().column,
        span.end().line,
        span.end().column,
    )
}

#[instrument(level = "debug", skip(syntax))]
#[hotpath::measure]
pub(super) fn collect(syntax: &syn::File) -> MacroTrees {
    let mut collector = MacroCollector::default();
    collector.visit_file(syntax);
    collector.trees
}

#[derive(Default)]
struct MacroCollector {
    trees: MacroTrees,
}

impl<'ast> Visit<'ast> for MacroCollector {
    #[instrument(level = "trace", skip(self, invocation))]
    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        let Some(body) = parse_payload(invocation) else {
            return;
        };
        match &body {
            SharedMacroSyntax::Expressions(expressions) => {
                for expr in expressions {
                    self.visit_expr(expr);
                }
            }
            SharedMacroSyntax::Items(items) | SharedMacroSyntax::TrailingItems(items) => {
                for item in items {
                    self.visit_item(item);
                }
            }
        }
        self.trees.insert(location(invocation), body);
    }
}

#[instrument(level = "trace", skip(invocation))]
fn parse_payload(invocation: &syn::Macro) -> Option<SharedMacroSyntax> {
    let path: Vec<_> = invocation
        .path
        .segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect();
    if path.last().is_some_and(|name| name == "format")
        || matches!(path.as_slice(), [name] if expression_arguments(name))
        || matches!(path.as_slice(), [root, name] if matches!(root.as_str(), "std" | "core") && expression_arguments(name))
    {
        let expressions = Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
            .parse2(invocation.tokens.clone())
            .ok()?;
        return Some(SharedMacroSyntax::Expressions(
            expressions.into_iter().collect(),
        ));
    }
    let tokens: Vec<_> = invocation.tokens.clone().into_iter().collect();
    let known_harness = matches!(path.as_slice(), [root, name] if root == "amenable_derive" && matches!(name.as_str(), "harness" | "gallery_harness"))
        && matches!(tokens.as_slice(), [TokenTree::Ident(_), TokenTree::Punct(first), TokenTree::Ident(_), TokenTree::Punct(second), TokenTree::Group(body)] if first.as_char() == ',' && second.as_char() == ',' && body.delimiter() == Delimiter::Brace);
    let body = tokens.iter().rev().find_map(|token| match token {
        TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => Some(group),
        _ => None,
    })?;
    let statements = syn::Block::parse_within.parse2(body.stream()).ok()?;
    let known_harness = known_harness
        && statements
            .iter()
            .all(|stmt| matches!(stmt, syn::Stmt::Item(_)));
    let items: Vec<_> = statements
        .into_iter()
        .filter_map(|statement| match statement {
            syn::Stmt::Item(item) => Some(item),
            _ => None,
        })
        .collect();
    if items.is_empty() && !known_harness {
        return None;
    }
    Some(if known_harness {
        SharedMacroSyntax::Items(items)
    } else {
        SharedMacroSyntax::TrailingItems(items)
    })
}

#[instrument(level = "trace")]
fn expression_arguments(name: &str) -> bool {
    matches!(
        name,
        "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
            | "vec"
            | "format"
            | "format_args"
            | "write"
            | "writeln"
    )
}
