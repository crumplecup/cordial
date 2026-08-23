//! Best-effort recovery of function bodies from inside `verus! { .. }`.
//!
//! `verus! { .. }` extends Rust's function-signature grammar with
//! `requires`/`ensures`/`decreases`/`recommends`/... clauses plain `syn`
//! can't parse, so the invocation's whole token stream -- or even one
//! function's own signature -- isn't valid Rust as far as `syn` is
//! concerned. `syn::visit::Visit` never descends into a macro's own
//! token stream at all (the same reason `kani_reach.rs`'s own
//! `items_inside_macro` exists for `amenable_derive::harness! { .. }`),
//! so without this, every real `panic!`/`unreachable!`/`.expect(..)`/
//! `.unwrap(..)` site inside any `verus! { .. }` block is invisible to
//! the panics scan entirely -- not exempted, just never seen.
//!
//! The function *body* (the final `{ .. }` block) is a different story
//! from its signature: Verus's own DSL (`assert(..)`, `assume(..)`, the
//! `requires`/`ensures` clauses themselves) is deliberately designed to
//! look like ordinary Rust wherever the macro's own expansion allows it,
//! specifically so real Rust tooling (rustfmt, rust-analyzer, and here,
//! `syn`) can still make sense of it without understanding Verus
//! semantics. Real Verus-only expression syntax (`x@`, `forall|x: int|
//! ..`, `nat`/`int` literals in certain positions) does still exist and
//! isn't parseable by plain `syn` -- recovery here is best-effort,
//! exactly like `items_inside_macro`'s own precedent: a body that fails
//! to parse is silently skipped, not reported as an error. Under-
//! detection here is strictly better than today's total blindness
//! inside every `verus!` block, never worse.

use proc_macro2::{Delimiter, TokenStream, TokenTree};

use tracing::instrument;

/// One function-shaped chunk recovered from inside a `verus! { .. }`
/// token stream: its real name, and its own body's raw, still-unparsed
/// tokens (real spans preserved, tied back to the original source file).
pub(super) struct VerusFunctionChunk {
    pub(super) name: String,
    pub(super) body: TokenStream,
}

/// Recover every function-shaped chunk inside `tokens`, at any nesting
/// depth (so a `verus! { impl Foo { fn bar() { .. } } }` shape still
/// finds `bar`). Does not descend into a chunk's own recovered body --
/// once a chunk is found, its body either parses as a real `syn::Block`
/// (in which case ordinary `syn::Visit` recursion already finds anything
/// nested inside it) or it doesn't, in which case the caller can retry
/// this same function on that one chunk's body tokens to still
/// opportunistically recover a nested `fn`.
#[instrument(level = "trace", skip(tokens))]
pub(super) fn collect_verus_functions(tokens: TokenStream) -> Vec<VerusFunctionChunk> {
    let mut out = Vec::new();
    let mut iter = tokens.into_iter().peekable();
    while let Some(tree) = iter.next() {
        match tree {
            TokenTree::Ident(ident) if ident == "fn" => {
                let Some(TokenTree::Ident(name)) = iter.next() else {
                    continue;
                };
                if let Some(body) = advance_to_body(&mut iter) {
                    out.push(VerusFunctionChunk {
                        name: name.to_string(),
                        body,
                    });
                }
            }
            TokenTree::Group(group) => {
                out.extend(collect_verus_functions(group.stream()));
            }
            _ => {}
        }
    }
    out
}

/// Consume tokens from `iter` up to and including the first
/// brace-delimited `Group` -- the function body -- skipping over
/// anything else along the way (the parameter list's own paren group, a
/// `(result: T)`-shaped named return binder, bare `requires`/`ensures`/
/// `decreases`/... clauses, which have no delimiter of their own in
/// Verus's grammar). Returns `None` if the stream runs out first
/// (malformed or truncated input -- e.g. a `fn` appearing in a
/// fn-pointer type with no body of its own to find).
#[instrument(level = "trace", skip(iter))]
fn advance_to_body(
    iter: &mut std::iter::Peekable<proc_macro2::token_stream::IntoIter>,
) -> Option<TokenStream> {
    for tree in iter.by_ref() {
        if let TokenTree::Group(group) = &tree
            && group.delimiter() == Delimiter::Brace
        {
            return Some(group.stream());
        }
    }
    None
}
