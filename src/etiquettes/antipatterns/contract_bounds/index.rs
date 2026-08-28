//! Registered contract fragments and call-shape matching.

use std::collections::HashMap;

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// One registered `amenable_core::Ensures`/`Requires` contract fragment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractRecordDump {
    pub evidence: String,
    pub verifier: String,
    pub kind: String,
    pub fragment: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct RegistryDump {
    #[serde(default)]
    pub(super) contract_records: Vec<ContractRecordDump>,
}

/// Registered contract records indexed by `(verifier, kind)` for lookup —
/// each entry keeps both `evidence` (the contract type's name, for Kani's
/// type-prefix suffix match) and `fragment` (the predicate's own source
/// text, for Creusot/Verus's callable-name extraction).
pub(super) struct ContractIndex {
    records: ContractRecordMap,
}

/// `(verifier, kind) -> [(evidence, fragment)]`.
type ContractRecordMap = HashMap<(String, String), Vec<(String, String)>>;

impl ContractIndex {
    #[instrument(level = "debug", skip(records))]
    pub(super) fn build(records: &[ContractRecordDump]) -> Self {
        let mut by_key: ContractRecordMap = HashMap::new();
        for record in records {
            by_key
                .entry((record.verifier.clone(), record.kind.clone()))
                .or_default()
                .push((record.evidence.clone(), record.fragment.clone()));
        }
        Self { records: by_key }
    }

    /// Whether `clause` is a real call to some registered contract's
    /// predicate — not a text-equality check, a call-shape recognition
    /// check. Two shapes:
    ///
    /// - **`<TypePath>::ensures(...)`/`<TypePath>::requires(...)`**
    ///   (Kani): the call's own path has the trailing `ensures`/`requires`
    ///   segment matching `kind`, so the type prefix (every segment
    ///   before it) is compared, turbofish-stripped, against every
    ///   registered `evidence` string for this `(verifier, kind)` — a
    ///   *suffix* match, since a call site's type name is usually
    ///   abbreviated by a `use` import while `evidence` is always fully
    ///   qualified. The fully-qualified disambiguating form, `<Type as
    ///   Ensures<V>>::ensures(...)` (`syn` represents this as a `Path`
    ///   with `qself: Some(..)`, not extra leading segments), is a
    ///   distinct sub-shape: the real type lives in `qself.ty`, never in
    ///   `path`'s own segments (those name the *trait*, e.g.
    ///   `Ensures<KaniVerifier>`) — matched directly against `qself.ty`
    ///   when present, no turbofish-stripping needed (type position
    ///   never writes `::<>`).
    /// - **`name(...)`** (Creusot/Verus): a bare single-segment call.
    ///   `name` is compared against the function name found in each
    ///   registered fragment's own `harness!`-captured source (scanned
    ///   for a literal `fn <name>` token pair, not parsed as a full
    ///   item — Verus's `pub open spec fn foo(...)` isn't valid Rust
    ///   grammar for `syn::ItemFn`, but "fn" is still just a plain
    ///   token to look for). A fragment that isn't a real function
    ///   definition — still a raw restated expression under the hood,
    ///   never actually wired to a shared predicate — yields no name and
    ///   never matches here, which is the *correct* outcome: that site
    ///   was never really using a named contract, only passing the old
    ///   text-equality check by coincidence.
    ///
    /// Kani still relies on real Rust-expression parsing, because its
    /// named-call shape needs a typed path prefix (`Type::ensures(...)`)
    /// rather than just a bare function name. Creusot/Verus can be more
    /// permissive: a top-level `name(...)` or `!name(...)` is recognized
    /// directly from tokens, so Verus-specific argument syntax like
    /// `final(self)` doesn't block a genuine named call from matching.
    ///
    /// Anything else that isn't a whole call at the clause boundary
    /// (most Pearlite, or expressions that merely *contain* a call)
    /// never matches — the caller's `is_trivial` check is the only other
    /// way a clause is allowed to go unnamed.
    /// A leading `!` is stripped before matching either shape:
    /// `assert!(!Type::ensures(value), "message")` is the idiomatic way
    /// to write a rejection-precondition check — still a real call to the
    /// registered `ensures` predicate. Only a single leading `!` is stripped.
    #[instrument(level = "debug", skip(self, clause))]
    pub(super) fn matches_named_call(
        &self,
        verifier: &str,
        kind: &str,
        clause: TokenStream,
    ) -> bool {
        let Some(known) = self.records.get(&(verifier.to_string(), kind.to_string())) else {
            return false;
        };

        if verifier != "kani"
            && let Some(name) = bare_named_call_name(clause.clone())
        {
            return known
                .iter()
                .any(|(_, fragment)| fragment_fn_name(fragment).as_deref() == Some(name.as_str()));
        }

        let Ok(expr) = syn::parse2::<syn::Expr>(clause) else {
            return false;
        };
        let call = match &expr {
            syn::Expr::Call(call) => call,
            syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Not(_)) => {
                match unary.expr.as_ref() {
                    syn::Expr::Call(call) => call,
                    _ => return false,
                }
            }
            _ => return false,
        };
        let syn::Expr::Path(func_path) = call.func.as_ref() else {
            return false;
        };
        let segments = &func_path.path.segments;
        let Some(last) = segments.last() else {
            return false;
        };

        if last.ident == kind {
            // `<Type as Trait>::ensures(...)` -- the fully-qualified
            // form, used deliberately in this workspace to disambiguate
            // when a second verifier registers a competing `Ensures`/
            // `Requires` impl for the same type (see `amenable`'s own
            // `CONTRACT_BOUND_NAMING_WORKFLOW.md` Gotchas). `path`'s own
            // segments here name the *trait* (`Ensures<KaniVerifier>`),
            // never the `Self` type a registered contract's `evidence`
            // describes -- the real type lives in `qself.ty` instead,
            // syn's dedicated slot for the part before `as`.
            if let Some(qself) = &func_path.qself {
                let prefix_text = normalize_tokens(qself.ty.to_token_stream());
                return known
                    .iter()
                    .any(|(evidence, _)| normalize_text(evidence).ends_with(&prefix_text));
            }

            if segments.len() >= 2 {
                let mut prefix = syn::Path {
                    leading_colon: func_path.path.leading_colon,
                    segments: syn::punctuated::Punctuated::new(),
                };
                for seg in segments.iter().take(segments.len() - 1) {
                    prefix.segments.push(seg.clone());
                }
                let prefix_text = strip_turbofish(&normalize_tokens(prefix.to_token_stream()));
                return known
                    .iter()
                    .any(|(evidence, _)| normalize_text(evidence).ends_with(&prefix_text));
            }
        }

        if segments.len() == 1 && func_path.qself.is_none() {
            let name = last.ident.to_string();
            return known
                .iter()
                .any(|(_, fragment)| fragment_fn_name(fragment).as_deref() == Some(name.as_str()));
        }

        false
    }
}

/// Drop the `::` a call-site turbofish (`Type::<Args>`) writes before its
/// generic argument list — `evidence` strings are always written in
/// plain type position (`Type<Args>`, no turbofish), so the two need the
/// same rendering before a suffix comparison means anything.
#[instrument(level = "debug")]
fn strip_turbofish(text: &str) -> String {
    text.replace(" :: <", " <")
}

/// Find the identifier immediately following a top-level `fn` token in
/// `fragment`'s own source text, without requiring the text to parse as
/// a complete, valid Rust item — Verus's `pub open spec fn foo(...)`
/// has real keywords (`open`, `spec`) `syn::ItemFn` doesn't accept, but
/// at the token level "fn" is still just a plain identifier to scan for.
/// Only the top-level token sequence is scanned (never descending into
/// a nested `Group`), so a helper function nested inside the fragment's
/// own body is never mistaken for the fragment's own name — and a doc
/// comment mentioning "fn" in prose can't produce a false match either,
/// since `///` lines tokenize into a single opaque `#[doc = "..."]`
/// string-literal token, not separate identifiers.
#[instrument(level = "debug")]
fn fragment_fn_name(fragment: &str) -> Option<String> {
    let tokens: TokenStream = fragment.parse().ok()?;
    let items: Vec<TokenTree> = tokens.into_iter().collect();
    items.windows(2).find_map(|pair| {
        let (TokenTree::Ident(keyword), TokenTree::Ident(name)) = (&pair[0], &pair[1]) else {
            return None;
        };
        (keyword == "fn").then(|| name.to_string())
    })
}

/// Recognize a whole-clause bare call `name(...)` or `!name(...)`
/// directly from tokens without requiring the argument list to parse as
/// plain Rust syntax. This is the Creusot/Verus call shape; Kani uses a
/// separate typed-path form handled through `syn::Expr`.
#[instrument(level = "debug", skip(clause))]
fn bare_named_call_name(clause: TokenStream) -> Option<String> {
    let items: Vec<TokenTree> = clause.into_iter().collect();
    let items = match items.as_slice() {
        [TokenTree::Punct(punct), rest @ ..] if punct.as_char() == '!' => rest,
        rest => rest,
    };

    match items {
        [TokenTree::Ident(name), TokenTree::Group(group)]
            if group.delimiter() == Delimiter::Parenthesis =>
        {
            Some(name.to_string())
        }
        _ => None,
    }
}

/// Which verifier a crate name maps to, if any — the only crates this rule
/// applies to.
#[instrument(level = "debug")]
pub(super) fn verifier_for_crate(crate_name: &str) -> Option<&'static str> {
    match crate_name {
        "amenable_creusot" => Some("creusot"),
        "amenable_verus" => Some("verus"),
        "amenable_kani" => Some("kani"),
        _ => None,
    }
}

/// Re-tokenize and re-stringify a fragment or clause into a canonical,
/// whitespace-normalized form. Tokenizing (not parsing as an expression)
/// is what makes this work for Pearlite/Verus-spec syntax that isn't valid
/// plain Rust.
///
/// Also splits any adjacent `>` run apart ([`split_adjacent_gt`]) so
/// suffix comparison succeeds against nested-generic `evidence` strings.
#[instrument(level = "debug")]
fn normalize_text(text: &str) -> String {
    text.parse::<TokenStream>()
        .map(|stream| split_adjacent_gt(&stream.to_string()))
        .unwrap_or_else(|_| text.trim().to_string())
}

/// Insert a space between every pair of immediately-adjacent `>` characters.
#[instrument(level = "debug")]
fn split_adjacent_gt(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '>' && chars.peek() == Some(&'>') {
            out.push(' ');
        }
    }
    out
}

#[instrument(level = "debug", skip(tokens))]
pub(super) fn normalize_tokens(tokens: TokenStream) -> String {
    tokens.to_string()
}

/// A bare `true`/`false` clause is the tautological case this project's
/// contract types already treat as needing no name — never flagged. A
/// bare `result` (Creusot's implicit binding, or Verus's explicit `->
/// (result: T)` one) is the same shape for a different reason: it isn't
/// a relationship between an external bound and a named type at all —
/// it's the backend's own mechanism for "the returned value is exactly
/// its own claim," with whatever real logic backs that claim living in
/// the function body, not the clause. Confirmed against real sites (e.g.
/// `verify_backtrace_force_capture_always_actually_captures`): the
/// interesting content is a `match` in the body computing the bool, not
/// anything the clause itself could name.
///
/// A bare tuple projection of `result` (`result.0`, `!result.3`, ...) is
/// the identical idiom for a tuple-returning function that packs several
/// boolean claims into one return value, one flag per position — same
/// "trust the body" shape, confirmed against real sites (e.g.
/// `verify_cell_model_get_set_replace_round_trip`'s `result.0, result.1,
/// result.2, result.3,`). A *comparison* against a projection
/// (`result.0 == value`) is not this idiom — that names a real
/// relationship and is still flagged.
///
/// `result.N is None` is the same idiom under a different spelling:
/// unlike `result.N == Some(x)` (whose right side varies with real
/// content per site), `is None` carries no argument at all — it's a
/// fixed sentinel check purely about what the model function's own body
/// already put in that position (the Verus iterator-exhaustion-model
/// convention: yield a fixed number of representative items, then
/// `None`), not a relationship to anything external the clause could
/// name. Confirmed against real sites recurring across many unrelated
/// iterator carriers (`iter_stateful_carrier`, `iter_sequence_carrier`,
/// `vec_deque_carrier`, ...) — the same tuple-position convention, not a
/// shared claim about those types themselves.
#[instrument(level = "trace", ret)]
pub(super) fn is_trivial(normalized: &str) -> bool {
    normalized == "true"
        || normalized == "false"
        || normalized == "result"
        || normalized == "! result"
        || is_bare_result_projection(normalized)
        || is_bare_result_is_none(normalized)
}

/// Whether `normalized` is exactly `result . N` or `! result . N` for
/// some decimal tuple index `N` — nothing else appended.
#[instrument(level = "trace", ret)]
fn is_bare_result_projection(normalized: &str) -> bool {
    let rest = normalized.strip_prefix("! ").unwrap_or(normalized);
    rest.strip_prefix("result . ")
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether `normalized` is exactly `result . N is None` for some decimal
/// tuple index `N` — nothing else appended.
#[instrument(level = "trace", ret)]
fn is_bare_result_is_none(normalized: &str) -> bool {
    normalized
        .strip_prefix("result . ")
        .and_then(|rest| rest.strip_suffix(" is None"))
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
}

/// Split a flat top-level token sequence on top-level commas. Safe by
/// construction: `TokenStream` iteration only ever yields top-level
/// `TokenTree`s — a comma inside a nested `Group` (e.g. `contains(&value)`)
/// is inside that `Group`'s own inner stream, never seen at this level.
#[instrument(level = "debug", skip(tokens))]
pub(super) fn split_top_level_commas(tokens: &[TokenTree]) -> Vec<Vec<TokenTree>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    for tt in tokens {
        if let TokenTree::Punct(punct) = tt
            && punct.as_char() == ','
        {
            segments.push(std::mem::take(&mut current));
            continue;
        }
        current.push(tt.clone());
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}
