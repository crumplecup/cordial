//! Registered contract fragments and call-shape matching.

use std::collections::HashMap;

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// One registered `amenable_core::Ensures`/`Requires` contract fragment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractRecordDump {
    /// Supporting evidence paths or labels.
    pub evidence: String,
    /// Proof verifier this row is about (`kani`, `creusot`, …).
    pub verifier: String,
    /// Contract kind (`ensures`, `requires`, …).
    pub kind: String,
    /// Source fragment of the contract bound.
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
                let prefix_text =
                    canonicalize_type_text(&normalize_tokens(qself.ty.to_token_stream()));
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
                let prefix_text = canonicalize_type_text(&strip_turbofish(&normalize_tokens(
                    prefix.to_token_stream(),
                )));
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

    /// Whether `clause` (found at `idx` within `siblings`, the full
    /// clause list split from the same `requires`/`ensures` occurrence)
    /// is a raw restatement of some *other* clause in `siblings` that is
    /// itself a real named call to a registered contract fragment.
    ///
    /// Verus's automatic broadcast/trigger instantiation needs a
    /// `#[trigger]`-marked equation to be the literal term appearing in
    /// the proof state — a call wrapping the same equation inside a named
    /// predicate gives the solver nothing to pattern-match on. This
    /// project's convention (confirmed via two real, independently
    /// doc-commented sites — `cstring_carrier.rs`'s
    /// `axiom_vec_u8_into_vec_u8_is_identity` and `cow_carrier.rs`'s
    /// `axiom_i32_to_owned_is_identity`) is to state the claim twice in
    /// the same `ensures` list: once as a raw `#[trigger]`ed equation for
    /// the solver, once as a bare named call for the reader and the
    /// registry. The raw half isn't a second, unnamed bound — it's a
    /// required-by-Verus restatement of the named one right next to it.
    ///
    /// Matching is strict, not heuristic: `clause`, with every one of its
    /// own `#[trigger]` attributes stripped (a clause can carry more
    /// than one — a comparison can mark each side separately), must be
    /// *token-identical* after normalization to the named sibling's own
    /// registered fragment body. A raw clause with no token-identical
    /// named sibling is still flagged — distinguishing this restatement
    /// idiom from a genuinely new, still-unnamed bound needs exact
    /// equality, not a loose "something nearby looks related" check that
    /// could mask a real future violation.
    ///
    /// The comparison runs through [`canonicalize_type_text`] (whitespace
    /// stripped), not plain [`normalize_tokens`] equality — the same
    /// reason [`Self::matches_named_call`]'s own type-prefix suffix match
    /// does: a fragment's body text is re-lexed from a plain string
    /// (`TokenStream::parse`), while a raw clause's own tokens come from
    /// parsing the real source file directly, and the two can pick
    /// different Joint/Alone spacing for identical-looking output.
    /// Confirmed real, not theoretical, on this exact check: a turbofish
    /// body (`type_id_carrier.rs`'s `i32_and_bool_type_ids_differ`,
    /// `type_id_spec::<i32>() != type_id_spec::<bool>()`) re-lexes from
    /// its registered fragment string as `type_id_spec :: < i32 > ()`
    /// (spaced) but parses from real source as `type_id_spec ::< i32 >
    /// ()` (unspaced) — identical after whitespace stripping, distinct
    /// under plain `.to_string()` equality.
    #[instrument(level = "debug", skip(self, clause, siblings))]
    pub(super) fn is_raw_duplicate_of_named_sibling(
        &self,
        verifier: &str,
        kind: &str,
        clause: TokenStream,
        siblings: &[TokenStream],
        idx: usize,
    ) -> bool {
        let own_normalized = canonicalize_type_text(&normalize_tokens(strip_trigger_attrs(clause)));
        siblings.iter().enumerate().any(|(sibling_idx, sibling)| {
            sibling_idx != idx
                && named_call_name_allowing_leading_attr(sibling.clone()).is_some_and(|name| {
                    self.named_fragment_body(verifier, kind, &name)
                        .is_some_and(|body| canonicalize_type_text(&body) == own_normalized)
                })
        })
    }

    /// The normalized body text of the registered `(verifier, kind)`
    /// fragment whose own `fn` name is `name`, if any — the counterpart
    /// [`fragment_fn_name`] needs to look inside a fragment's body rather
    /// than just its name, for [`Self::is_raw_duplicate_of_named_sibling`].
    #[instrument(level = "debug", skip(self))]
    fn named_fragment_body(&self, verifier: &str, kind: &str, name: &str) -> Option<String> {
        let known = self
            .records
            .get(&(verifier.to_string(), kind.to_string()))?;
        known.iter().find_map(|(_, fragment)| {
            (fragment_fn_name(fragment).as_deref() == Some(name))
                .then(|| fragment_fn_body_text(fragment))
                .flatten()
        })
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

/// Find the brace-delimited body immediately following the top-level
/// `fn <name>` pair in `fragment`'s own source text, normalized the same
/// way a clause's own tokens are (see [`normalize_tokens`]) — the
/// counterpart to [`fragment_fn_name`], which finds the name instead of
/// the body. Used only by [`ContractIndex::is_raw_duplicate_of_named_sibling`]
/// to compare a registered predicate's real body against a raw clause
/// that claims to restate it.
#[instrument(level = "debug")]
fn fragment_fn_body_text(fragment: &str) -> Option<String> {
    let tokens: TokenStream = fragment.parse().ok()?;
    let items: Vec<TokenTree> = tokens.into_iter().collect();
    let fn_idx = items
        .windows(2)
        .position(|pair| matches!(&pair[0], TokenTree::Ident(keyword) if keyword == "fn"))?;
    items[fn_idx..].iter().find_map(|tt| match tt {
        TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
            Some(normalize_tokens(group.stream()))
        }
        _ => None,
    })
}

/// Strip every top-level `#[trigger]` attribute from `tokens`. Verus's
/// raw-equation restatement clause (see
/// [`ContractIndex::is_raw_duplicate_of_named_sibling`]) can carry more
/// than one — a comparison between two independently-triggered subterms
/// (`#[trigger] a() != #[trigger] b()`, confirmed real:
/// `type_id_carrier.rs`'s `axiom_i32_and_bool_type_ids_differ`) marks
/// each side separately, not just the clause's own leading token. The
/// registered named predicate's own body never carries any `#[trigger]`
/// attribute, so comparing the two for equality needs every one
/// stripped, not just a leading one. Only a bare, argument-less
/// `#[trigger]` is recognized — Verus's other `#![trigger a, b]`
/// multi-term statement-level syntax is a different construct entirely
/// and is left untouched. Only the top-level token sequence is scanned
/// (never descending into a nested `Group`), matching every other
/// token-shape helper in this module.
#[instrument(level = "debug", skip(tokens))]
fn strip_trigger_attrs(tokens: TokenStream) -> TokenStream {
    let items: Vec<TokenTree> = tokens.into_iter().collect();
    let mut out = Vec::with_capacity(items.len());
    let mut i = 0;
    while i < items.len() {
        if let (TokenTree::Punct(hash), Some(TokenTree::Group(group))) =
            (&items[i], items.get(i + 1))
            && hash.as_char() == '#'
            && group.delimiter() == Delimiter::Bracket
            && group.stream().to_string() == "trigger"
        {
            i += 2;
            continue;
        }
        out.push(items[i].clone());
        i += 1;
    }
    out.into_iter().collect()
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

/// [`bare_named_call_name`], extended to also recognize a Creusot/Verus
/// bare call carrying a leading outer attribute (`#[trigger] name(...)`,
/// Verus's own solver-hint annotation on a broadcast axiom's *named*
/// sibling clause — real Rust expression grammar allows an outer
/// attribute here, but `bare_named_call_name`'s pure-token scan doesn't
/// look past one). Falls back to a real `syn::Expr::Call` parse, which
/// happily accepts the leading attribute as part of the expression and
/// still exposes the call underneath; only used by
/// [`ContractIndex::is_raw_duplicate_of_named_sibling`], which needs to
/// resolve a sibling clause's call name exactly as permissively as
/// [`ContractIndex::matches_named_call`] already does for that same
/// clause when it's checked directly (confirmed necessary: a two-clause
/// `ensures` list where *both* clauses carry `#[trigger]` -- real site,
/// `cstring_carrier.rs`'s `axiom_vec_u8_into_vec_u8_is_identity` --
/// otherwise makes the named sibling invisible to this check even
/// though `matches_named_call` itself already accepts it).
#[instrument(level = "debug", skip(clause))]
fn named_call_name_allowing_leading_attr(clause: TokenStream) -> Option<String> {
    if let Some(name) = bare_named_call_name(clause.clone()) {
        return Some(name);
    }
    let expr = syn::parse2::<syn::Expr>(clause).ok()?;
    let call = match &expr {
        syn::Expr::Call(call) => call,
        syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Not(_)) => {
            match unary.expr.as_ref() {
                syn::Expr::Call(call) => call,
                _ => return None,
            }
        }
        _ => return None,
    };
    let syn::Expr::Path(func_path) = call.func.as_ref() else {
        return None;
    };
    (func_path.qself.is_none() && func_path.path.segments.len() == 1)
        .then(|| func_path.path.segments.last().unwrap().ident.to_string())
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

/// Re-tokenize and re-stringify a fragment or clause into a canonical
/// form for the type-prefix suffix comparison in
/// [`ContractIndex::matches_named_call`]. Tokenizing (not parsing as an
/// expression) is what makes this work for Pearlite/Verus-spec syntax
/// that isn't valid plain Rust.
///
/// Passed through [`canonicalize_type_text`] rather than merely
/// whitespace-*normalized*: `evidence` strings are re-lexed from plain
/// text (`TokenStream::parse`), while a call site's type prefix comes
/// from `ToTokens` on a live `syn::Path`/`syn::Type` AST — the two can
/// pick different Joint/Alone spacing for identical-looking output even
/// after generic normalization (confirmed against a nested-generic type
/// carrying a lifetime, `Bytes<'static>`: one path prints
/// `Bytes <'static > >`, the other `Bytes < 'static > >`, an
/// inconsistency in the space *before* the lifetime's leading `'`, on
/// top of the adjacent-`>>` case a single space-insertion pass already
/// had to special-case).
#[instrument(level = "debug")]
fn normalize_text(text: &str) -> String {
    text.parse::<TokenStream>()
        .map(|stream| canonicalize_type_text(&stream.to_string()))
        .unwrap_or_else(|_| canonicalize_type_text(text.trim()))
}

/// Canonicalize `text` for the type-prefix suffix comparison in
/// [`ContractIndex::matches_named_call`]: strip every whitespace
/// character (whitespace carries no meaning in a suffix comparison over
/// Rust tokens, so stripping it entirely removes a whole class of
/// spacing mismatches at once rather than patching one quirk at a time
/// -- see [`normalize_text`]), then collapse an elidable trailing comma
/// before a closing `>` in a generic-argument list. `Type<A, B,>` and
/// `Type<A, B>` are the same type — Rust's grammar makes a trailing
/// comma in a turbofish/generic-argument list optional — but `syn`'s
/// `Punctuated` preserves a source trailing comma through `ToTokens`
/// when the call site writes one (confirmed against a real multi-line
/// call site, `RustStdStandard::<\n    ExtractIf<'static, i32, fn(&mut
/// i32) -> bool>,\n>::ensures(..)`, whose re-emitted prefix ends
/// `...bool>,>` against a hand-written `evidence` string ending
/// `...bool>>` — an otherwise-correct suffix match failing on a comma
/// with no semantic meaning). Collapsing every `,>` to `>` in one pass
/// is safe: a comma immediately before a generic list's closing `>` is
/// never anything other than an elidable trailing separator.
#[instrument(level = "debug")]
fn canonicalize_type_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace(",>", ">")
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

/// Verus's own builtin function-item contract-inspection syntax --
/// `<expr>.ensures(...)`/`<expr>.requires(...)` (e.g. `H::default.ensures
/// ((), result)`, inspecting a generic parameter's own real contract) --
/// spelled identically to the real clause-list keyword, but a genuine
/// method call on some function-item value, not a name this project
/// could ever mint a local predicate for: it inspects an *external*
/// function's own contract, which by definition has no local `fn` to
/// point at. `preceded_by_dot` (`verus.rs`) already tells this shape
/// apart from the real keyword while walking raw tokens (both spell
/// `ensures`/`requires` identically); this is the same distinction
/// applied to a fully parsed clause instead of a bare token, so it
/// doesn't need a registered fragment at all -- a real
/// `syn::Expr::MethodCall` whose own method name is exactly `kind` is
/// this shape, unconditionally, no registry lookup involved.
#[instrument(level = "debug", skip(clause))]
pub(super) fn is_builtin_contract_inspection(kind: &str, clause: TokenStream) -> bool {
    let Ok(syn::Expr::MethodCall(method_call)) = syn::parse2::<syn::Expr>(clause) else {
        return false;
    };
    method_call.method == kind
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
