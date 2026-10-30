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
            SharedMacroSyntax::Items(items) => {
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
    if matches!(path.as_slice(), [name] if expression_arguments(name))
        || matches!(path.as_slice(), [root, name] if matches!(root.as_str(), "std" | "core") && expression_arguments(name))
    {
        let expressions = Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
            .parse2(invocation.tokens.clone())
            .ok()?;
        return Some(SharedMacroSyntax::Expressions(
            expressions.into_iter().collect(),
        ));
    }
    if !matches!(path.as_slice(), [root, name] if root == "amenable_derive" && matches!(name.as_str(), "harness" | "gallery_harness"))
    {
        return None;
    }
    let tokens: Vec<_> = invocation.tokens.clone().into_iter().collect();
    let [
        TokenTree::Ident(_),
        TokenTree::Punct(first),
        TokenTree::Ident(_),
        TokenTree::Punct(second),
        TokenTree::Group(body),
    ] = tokens.as_slice()
    else {
        return None;
    };
    if first.as_char() != ',' || second.as_char() != ',' || body.delimiter() != Delimiter::Brace {
        return None;
    }
    let syntax = syn::parse2::<syn::File>(body.stream()).ok()?;
    Some(SharedMacroSyntax::Items(syntax.items))
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
    )
}
