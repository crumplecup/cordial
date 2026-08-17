//! Detect requires/ensures sites whose expression matches no registered
//! `amenable_core::Ensures`/`Requires` contract fragment for its verifier —
//! the raw-equation antipattern `AntipatternRuleId::UnnamedContractBound001`
//! flags.
//!
//! Amenable-specific, unlike this module's other three rules: it only
//! applies to `amenable_creusot`/`amenable_verus`/`amenable_kani`, and it
//! needs real data from `amenable dump-registry`'s `contract_records`
//! (see `framework_std::registry`) to check against, rather than being a
//! purely self-contained syntactic probe.
//!
//! Each backend states a requires/ensures bound in a different shape:
//!
//! - **Creusot**: `#[requires(...)]`/`#[ensures(...)]` attributes on real
//!   `fn`/`impl fn` items — ordinary Rust attribute syntax, but their
//!   *contents* are Pearlite, not plain Rust (`c@ <= 0xD7FF`, `==>`, …),
//!   which plain `syn::Expr` parsing rejects. Comparison here works at the
//!   token level, not the expression-AST level, specifically to avoid
//!   needing to understand Pearlite's grammar.
//! - **Verus**: `requires ..., ensures ...,` clauses live inside a
//!   `verus! { ... }` function-like macro invocation. `syn::parse_file`
//!   succeeds on the whole file (a macro body is always an opaque token
//!   group to `syn`), but the clauses themselves are invisible to ordinary
//!   `syn::visit::Visit` — they're walked directly as a raw
//!   `proc_macro2::TokenStream` instead.
//! - **Kani**: proofs have no requires/ensures attribute at all — the same
//!   bound is expressed as `assert!(EXPR, ..)` or `assert_eq!(A, B, ..)`
//!   (ensures-equivalent; `assert_eq!` synthesizes the clause `A == B` —
//!   a direct transcription of its two comparands, not a guess) or
//!   `kani::assume(EXPR)` (requires-equivalent) inside a `#[kani::proof]`
//!   function body. `kani::assume` is a plain function call, not a macro
//!   invocation (`kani::assume` has no `!`), so it needs its own
//!   `syn::ExprCall` visitor rather than living alongside the
//!   `assert!`/`assert_eq!` macro-node visitor. A bound staged through an
//!   intermediate `let` binding before being asserted (as in
//!   `rust_std::char`'s own
//!   `verify_char_try_from_fails_exactly_for_surrogates_and_out_of_range`)
//!   is not detected — tracking that would need real dataflow analysis,
//!   not a syntactic probe.
//!
//! Any `gallery` directory (structural convention, not a per-project
//! config option) is pruned from the walk entirely: it holds verifier
//! experiments and documented dead ends, not production proofs, so its
//! raw clauses are never candidates for a named contract type.
//!
//! A clause is accepted not by matching its *text* against a registered
//! fragment, but by recognizing its *shape* as a real call to a
//! registered contract's predicate — see [`ContractIndex::matches_named_call`]
//! for the two call shapes this recognizes (`Type::ensures(...)` for
//! Kani, bare `name(...)` for Creusot/Verus). This deliberately replaced
//! an earlier text-equality design: matching literal call-site text
//! against a registered fragment needed a second, hand-typed
//! registration per distinct call shape purely to keep the scanner
//! quiet — ceremony that existed to satisfy the tool, not because it
//! caught anything the call-shape check doesn't already catch for real.
//! A bare `true`/`false` clause is never flagged — it is the
//! tautological case this project's own contract types already treat as
//! needing no name (see `amenable_core::Ensures`'s own doc comment).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Attribute, ImplItemFn, ItemFn, ItemMacro, Meta};
use tracing::instrument;
use walkdir::WalkDir;

use crate::error::{CordialError, CordialResult};
use crate::loader::module_path_from_src_file;
use crate::store::StoreLayout;

use super::scan::truncate_snippet;
use super::types::{AntipatternRuleId, AntipatternSiteRecord};

const AMENABLE_DUMP_REGISTRY_FEATURES: &str = "creusot,verus";

/// One registered `amenable_core::Ensures`/`Requires` contract fragment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractRecordDump {
    pub evidence: String,
    pub verifier: String,
    pub kind: String,
    pub fragment: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RegistryDump {
    #[serde(default)]
    contract_records: Vec<ContractRecordDump>,
}

static CONTRACT_RECORDS_CACHE: Mutex<Option<(PathBuf, Vec<ContractRecordDump>)>> = Mutex::new(None);

/// Registered contract records indexed by `(verifier, kind)` for lookup —
/// each entry keeps both `evidence` (the contract type's name, for Kani's
/// type-prefix suffix match) and `fragment` (the predicate's own source
/// text, for Creusot/Verus's callable-name extraction).
struct ContractIndex {
    records: ContractRecordMap,
}

/// `(verifier, kind) -> [(evidence, fragment)]`.
type ContractRecordMap = HashMap<(String, String), Vec<(String, String)>>;

impl ContractIndex {
    #[instrument(skip(records))]
    fn build(records: &[ContractRecordDump]) -> Self {
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
    ///   qualified.
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
    /// Anything that doesn't parse as a call at all (most Pearlite,
    /// which isn't valid Rust expression syntax) never matches — the
    /// caller's `is_trivial` check is the only other way a clause is
    /// allowed to go unnamed.
    /// A leading `!` is stripped before matching either shape:
    /// `assert!(!Type::ensures(value), "message")` is the idiomatic way
    /// to write a rejection-precondition check — still a real call to the
    /// registered `ensures` predicate. Only a single leading `!` is stripped.
    #[instrument(skip(self, clause))]
    fn matches_named_call(&self, verifier: &str, kind: &str, clause: TokenStream) -> bool {
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
        let Some(known) = self.records.get(&(verifier.to_string(), kind.to_string())) else {
            return false;
        };

        if segments.len() >= 2 && last.ident == kind {
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

        if segments.len() == 1 {
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
#[instrument]
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
#[instrument(skip(fragment))]
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

/// Which verifier a crate name maps to, if any — the only crates this rule
/// applies to.
#[instrument]
fn verifier_for_crate(crate_name: &str) -> Option<&'static str> {
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
#[instrument]
fn normalize_text(text: &str) -> String {
    text.parse::<TokenStream>()
        .map(|stream| split_adjacent_gt(&stream.to_string()))
        .unwrap_or_else(|_| text.trim().to_string())
}

/// Insert a space between every pair of immediately-adjacent `>` characters.
#[instrument]
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

#[instrument(skip(tokens))]
fn normalize_tokens(tokens: TokenStream) -> String {
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
#[instrument]
fn is_trivial(normalized: &str) -> bool {
    normalized == "true"
        || normalized == "false"
        || normalized == "result"
        || normalized == "! result"
        || is_bare_result_projection(normalized)
        || is_bare_result_is_none(normalized)
}

/// Whether `normalized` is exactly `result . N` or `! result . N` for
/// some decimal tuple index `N` — nothing else appended.
#[instrument]
fn is_bare_result_projection(normalized: &str) -> bool {
    let rest = normalized.strip_prefix("! ").unwrap_or(normalized);
    rest.strip_prefix("result . ")
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether `normalized` is exactly `result . N is None` for some decimal
/// tuple index `N` — nothing else appended.
#[instrument]
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
#[instrument(skip(tokens))]
fn split_top_level_commas(tokens: &[TokenTree]) -> Vec<Vec<TokenTree>> {
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

/// A `gallery` directory holds verifier experiments, not production
/// proofs (see e.g. `amenable_kani::gallery`'s own doc comment: "the
/// gallery answers a different question" than the production queue).
/// Its `assert!`/`#[ensures]`/`requires` clauses are deliberately
/// throwaway probes, not reusable bounds worth a named contract type,
/// so the whole subtree is pruned from this rule's walk rather than
/// flagged and then hand-excepted site by site.
#[instrument(skip(entry))]
fn is_gallery_dir(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir() && entry.file_name() == "gallery"
}

/// Scan one crate's `src/**/*.rs` tree for the backend it's written for.
/// Returns no findings for any other crate — this rule only applies to
/// the three verifier crates.
#[instrument(skip(registry), fields(crate_name, crate_root = %crate_root.display()))]
pub fn scan_crate_contract_bounds(
    crate_root: &Path,
    crate_name: &str,
    registry: &[ContractRecordDump],
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let Some(verifier) = verifier_for_crate(crate_name) else {
        return Ok(Vec::new());
    };

    let src_root = crate_root.join("src");
    if !src_root.is_dir() {
        return Ok(Vec::new());
    }

    let index = ContractIndex::build(registry);
    let mut findings = Vec::new();

    for entry in WalkDir::new(&src_root)
        .into_iter()
        .filter_entry(|e| !is_gallery_dir(e))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(path)?;
        let mut file_findings = match verifier {
            "creusot" => scan_creusot_source(&source, path, &src_root, crate_name, &index)?,
            "verus" => scan_verus_source(&source, path, &src_root, crate_name, &index)?,
            "kani" => scan_kani_source(&source, path, &src_root, crate_name, &index)?,
            _ => Vec::new(),
        };
        findings.append(&mut file_findings);
    }

    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.snippet.cmp(&b.snippet))
    });

    for finding in &mut findings {
        if let Ok(rel) = finding.file.strip_prefix(crate_root) {
            finding.file = rel.to_path_buf();
        }
    }

    Ok(findings)
}

#[instrument(skip(module_prefix))]
fn site_context(module_prefix: &[String], leaf: &str) -> String {
    let mut parts = module_prefix.to_vec();
    parts.push(leaf.to_string());
    parts.join("::")
}

#[instrument(skip(normalized))]
fn make_finding(
    _crate_name: &str,
    context: String,
    file: &Path,
    line: u32,
    normalized: &str,
) -> AntipatternSiteRecord {
    AntipatternSiteRecord {
        rule_id: AntipatternRuleId::UnnamedContractBound001,
        context,
        file: file.to_path_buf(),
        line,
        snippet: truncate_snippet(normalized, 96),
    }
}

/// Scan one Creusot source string (used by tests, mirroring
/// `scan_antipatterns_rust_source`'s shape for the other three rules).
#[instrument(skip(source, registry), fields(file = %file.display()))]
pub fn scan_creusot_contract_bounds_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    registry: &[ContractRecordDump],
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    scan_creusot_source(
        source,
        file,
        src_root,
        crate_name,
        &ContractIndex::build(registry),
    )
}

/// Scan one Verus source string (used by tests).
#[instrument(skip(source, registry), fields(file = %file.display()))]
pub fn scan_verus_contract_bounds_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    registry: &[ContractRecordDump],
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    scan_verus_source(
        source,
        file,
        src_root,
        crate_name,
        &ContractIndex::build(registry),
    )
}

/// Scan one Kani source string (used by tests).
#[instrument(skip(source, registry), fields(file = %file.display()))]
pub fn scan_kani_contract_bounds_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    registry: &[ContractRecordDump],
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    scan_kani_source(
        source,
        file,
        src_root,
        crate_name,
        &ContractIndex::build(registry),
    )
}

/// `amenable_derive::harness!(cfg_name, CONST_NAME, { item })` wraps nearly
/// every real Kani and Creusot proof in this workspace — `syn::parse_file`
/// sees the whole invocation as one opaque `Item::Macro`, the same blind
/// spot `verus!{}` has, so the wrapped `fn` is otherwise invisible to
/// `syn::visit::Visit`. Extracts and parses the trailing brace-delimited
/// group (`{ item }`) as a real `ItemFn`, so it can be fed back into the
/// same visitor logic that already handles top-level functions.
#[instrument(skip(node))]
fn harness_macro_item_fn(node: &ItemMacro) -> Option<ItemFn> {
    if node
        .mac
        .path
        .segments
        .last()
        .is_none_or(|seg| seg.ident != "harness")
    {
        return None;
    }
    let group = node
        .mac
        .tokens
        .clone()
        .into_iter()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find_map(|tt| {
            if let TokenTree::Group(group) = tt
                && group.delimiter() == Delimiter::Brace
            {
                Some(group)
            } else {
                None
            }
        })?;
    syn::parse2::<ItemFn>(group.stream()).ok()
}

// --- Creusot: #[requires(...)]/#[ensures(...)] attributes -----------------

#[instrument(skip(source, index), fields(file = %file.display()))]
fn scan_creusot_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    index: &ContractIndex,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = CreusotVisitor {
        crate_name: crate_name.to_string(),
        file: file.to_path_buf(),
        module_prefix,
        fn_stack: Vec::new(),
        index,
        findings: Vec::new(),
    };
    visitor.visit_file(&syntax);
    Ok(visitor.findings)
}

struct CreusotVisitor<'a> {
    crate_name: String,
    file: PathBuf,
    module_prefix: Vec<String>,
    fn_stack: Vec<String>,
    index: &'a ContractIndex,
    findings: Vec<AntipatternSiteRecord>,
}

impl CreusotVisitor<'_> {
    #[instrument(skip(self, attrs))]
    fn check_attrs(&mut self, attrs: &[Attribute]) {
        for attr in attrs {
            let kind = if attr.path().is_ident("requires") {
                "requires"
            } else if attr.path().is_ident("ensures") {
                "ensures"
            } else {
                continue;
            };
            let Meta::List(list) = &attr.meta else {
                continue;
            };
            let normalized = normalize_tokens(list.tokens.clone());
            if is_trivial(&normalized)
                || self
                    .index
                    .matches_named_call("creusot", kind, list.tokens.clone())
            {
                continue;
            }
            let context = site_context(&self.module_prefix, &self.fn_stack.join("::"));
            self.findings.push(make_finding(
                &self.crate_name,
                context,
                &self.file,
                attr.span().start().line as u32,
                &normalized,
            ));
        }
    }
}

impl<'ast> Visit<'ast> for CreusotVisitor<'_> {
    #[instrument(skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_attrs(&node.attrs);
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(skip(self, node))]
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        self.check_attrs(&node.attrs);
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }

    #[instrument(skip(self, node))]
    fn visit_item_macro(&mut self, node: &'ast ItemMacro) {
        if let Some(item_fn) = harness_macro_item_fn(node) {
            self.visit_item_fn(&item_fn);
        }
        syn::visit::visit_item_macro(self, node);
    }
}

// --- Verus: requires/ensures clauses inside verus! { ... } ----------------

#[instrument(skip(source, index), fields(file = %file.display()))]
fn scan_verus_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    index: &ContractIndex,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut findings = Vec::new();

    for item in &syntax.items {
        let syn::Item::Macro(ItemMacro { mac, .. }) = item else {
            continue;
        };
        if mac
            .path
            .segments
            .last()
            .is_none_or(|seg| seg.ident != "verus")
        {
            continue;
        }
        let mut clauses = Vec::new();
        walk_verus_tokens(mac.tokens.clone(), &mut clauses);
        for (kind, clause, harness) in clauses {
            let normalized = normalize_tokens(clause.clone());
            if is_trivial(&normalized) || index.matches_named_call("verus", kind, clause.clone()) {
                continue;
            }
            let line = clause
                .clone()
                .into_iter()
                .next()
                .map(|tt| tt.span().start().line as u32)
                .unwrap_or(0);
            let leaf = if harness.is_empty() {
                "verus!".to_string()
            } else {
                format!("verus!::{harness}")
            };
            let context = site_context(&module_prefix, &leaf);
            findings.push(make_finding(crate_name, context, file, line, &normalized));
        }
    }

    Ok(findings)
}

/// Recursively find every `requires`/`ensures` keyword in `tokens`, descending
/// into every nested `Group` (function bodies, parenthesized expressions,
/// ...) since a whole `verus!{}` file may contain many functions. For each
/// keyword found, the clause list runs from the token right after it up to
/// the next top-level brace group (the function body starting) or the next
/// `requires`/`ensures` keyword, split on top-level commas into individual
/// clauses.
///
/// Also tracks the enclosing `fn`'s name at the current nesting level (a
/// `requires`/`ensures` clause always sits directly after its own
/// function's signature, at the same top-level token sequence as the `fn`
/// keyword — never inside a nested `Group`), and attaches it to each
/// clause so matching can be scoped to "this clause's own site."
#[instrument(skip(tokens, out))]
fn walk_verus_tokens(tokens: TokenStream, out: &mut Vec<(&'static str, TokenStream, String)>) {
    let items: Vec<TokenTree> = tokens.into_iter().collect();
    let mut i = 0;
    let mut current_fn = String::new();
    while i < items.len() {
        match &items[i] {
            TokenTree::Ident(ident) if ident == "fn" => {
                if let Some(TokenTree::Ident(name)) = items.get(i + 1) {
                    current_fn = name.to_string();
                }
                i += 1;
            }
            TokenTree::Ident(ident) if ident == "requires" || ident == "ensures" => {
                let kind = if ident == "requires" {
                    "requires"
                } else {
                    "ensures"
                };
                let mut j = i + 1;
                while j < items.len() {
                    match &items[j] {
                        TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => break,
                        TokenTree::Ident(next) if next == "requires" || next == "ensures" => break,
                        _ => j += 1,
                    }
                }
                for segment in split_top_level_commas(&items[i + 1..j]) {
                    if segment.is_empty() {
                        continue;
                    }
                    out.push((kind, segment.into_iter().collect(), current_fn.clone()));
                }
                i = j;
            }
            TokenTree::Group(group) => {
                walk_verus_tokens(group.stream(), out);
                i += 1;
            }
            _ => i += 1,
        }
    }
}

// --- Kani: assert!(...)/kani::assume(...) inside #[kani::proof] fns -------

#[instrument(skip(source, index), fields(file = %file.display()))]
fn scan_kani_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    index: &ContractIndex,
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    let syntax = syn::parse_file(source)
        .map_err(|err| CordialError::syn_parse(file.display().to_string(), err))?;
    let module_prefix = module_path_from_src_file(src_root, file);
    let mut visitor = KaniVisitor {
        crate_name: crate_name.to_string(),
        file: file.to_path_buf(),
        module_prefix,
        fn_stack: Vec::new(),
        in_kani_proof: false,
        index,
        findings: Vec::new(),
        error: None,
    };
    visitor.visit_file(&syntax);
    if let Some(err) = visitor.error {
        return Err(err);
    }
    Ok(visitor.findings)
}

struct KaniVisitor<'a> {
    crate_name: String,
    file: PathBuf,
    module_prefix: Vec<String>,
    fn_stack: Vec<String>,
    in_kani_proof: bool,
    index: &'a ContractIndex,
    findings: Vec<AntipatternSiteRecord>,
    error: Option<CordialError>,
}

impl KaniVisitor<'_> {
    #[instrument(skip(attrs))]
    fn is_kani_proof(attrs: &[Attribute]) -> bool {
        attrs.iter().any(|attr| {
            attr.path()
                .segments
                .last()
                .is_some_and(|seg| seg.ident == "proof")
                && attr.path().segments.iter().any(|seg| seg.ident == "kani")
        })
    }

    /// Common tail for every Kani clause, whichever shape it was
    /// captured from: normalize, skip trivial/registered clauses, else
    /// record a finding.
    #[instrument(skip(self, clause))]
    fn check_clause(&mut self, kind: &str, clause: TokenStream, line: u32) {
        let normalized = normalize_tokens(clause.clone());
        if is_trivial(&normalized) || self.index.matches_named_call("kani", kind, clause) {
            return;
        }
        let context = site_context(&self.module_prefix, &self.fn_stack.join("::"));
        self.findings.push(make_finding(
            &self.crate_name,
            context,
            &self.file,
            line,
            &normalized,
        ));
    }

    /// `assert!(EXPR, ..)`/`assert_eq!(A, B, ..)` — both ensures-shaped.
    /// `assert_eq!` synthesizes the clause `A == B` from its two
    /// comparands: a direct transcription of what's on the page, not a
    /// guess.
    #[instrument(skip(self, node))]
    fn check_macro_call(&mut self, node: &syn::Macro) {
        if !self.in_kani_proof {
            return;
        }
        let path = &node.path;
        let items: Vec<TokenTree> = node.tokens.clone().into_iter().collect();
        let segments = split_top_level_commas(&items);

        let (first_span_tokens, clause): (&[TokenTree], TokenStream) = if path.is_ident("assert") {
            // Not a plain assert!(expr) or assert!(expr, "msg") shape —
            // skip rather than guess.
            if segments.len() > 2 {
                return;
            }
            let Some(expr) = segments.first() else {
                return;
            };
            (expr, expr.iter().cloned().collect())
        } else if path.is_ident("assert_eq") {
            if segments.len() < 2 {
                return;
            }
            let mut clause = TokenStream::new();
            clause.extend(segments[0].iter().cloned());
            clause.extend(match "==".parse::<TokenStream>() {
                Ok(tokens) => tokens,
                Err(err) => {
                    self.error = Some(err.into());
                    return;
                }
            });
            clause.extend(segments[1].iter().cloned());
            (&segments[0], clause)
        } else {
            return;
        };

        let line = first_span_tokens
            .first()
            .map(|tt| tt.span().start().line as u32)
            .unwrap_or_else(|| node.span().start().line as u32);
        self.check_clause("ensures", clause, line);
    }

    /// `kani::assume(EXPR)` — the requires-equivalent. Unlike
    /// `assert!`/`assert_eq!`, this is a plain function call (`assume`
    /// has no `!`), never a macro invocation, so it needs its own
    /// `syn::ExprCall` visitor rather than living in [`Self::check_macro_call`].
    #[instrument(skip(self, node))]
    fn check_assume_call(&mut self, node: &syn::ExprCall) {
        if !self.in_kani_proof {
            return;
        }
        let syn::Expr::Path(func) = node.func.as_ref() else {
            return;
        };
        let path = &func.path;
        let is_assume = path
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "assume")
            && path.segments.iter().any(|seg| seg.ident == "kani");
        if !is_assume {
            return;
        }
        let Some(expr) = node.args.first() else {
            return;
        };
        let line = expr.span().start().line as u32;
        self.check_clause("requires", expr.to_token_stream(), line);
    }
}

impl<'ast> Visit<'ast> for KaniVisitor<'_> {
    #[instrument(skip(self, node))]
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if self.error.is_some() {
            return;
        }
        let prev = self.in_kani_proof;
        self.in_kani_proof = Self::is_kani_proof(&node.attrs);
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
        self.in_kani_proof = prev;
    }

    #[instrument(skip(self, node))]
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self.error.is_some() {
            return;
        }
        self.check_macro_call(node);
        syn::visit::visit_macro(self, node);
    }

    #[instrument(skip(self, node))]
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.error.is_some() {
            return;
        }
        self.check_assume_call(node);
        syn::visit::visit_expr_call(self, node);
    }

    #[instrument(skip(self, node))]
    fn visit_item_macro(&mut self, node: &'ast ItemMacro) {
        if self.error.is_some() {
            return;
        }
        if let Some(item_fn) = harness_macro_item_fn(node) {
            self.visit_item_fn(&item_fn);
        }
        syn::visit::visit_item_macro(self, node);
    }
}

/// Load contract records for the unnamed-contract-bound rule.
pub fn fetch_contract_records(workspace_root: &Path, store_root: &Path) -> Vec<ContractRecordDump> {
    let cache_key = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());

    if let Ok(cache) = CONTRACT_RECORDS_CACHE.lock()
        && let Some((key, records)) = cache.as_ref()
        && *key == cache_key
    {
        return records.clone();
    }

    let slug = crate::store::project_slug_from_path(workspace_root);
    let store = StoreLayout::from_root(store_root, slug);
    let dump_path = store.cache_dir().join("amenable-registry.dump.json");

    let records = if dump_path.is_file() {
        load_registry_dump(&dump_path).unwrap_or_default()
    } else if workspace_has_amenable(workspace_root) {
        if run_amenable_dump_registry(workspace_root, &dump_path).is_ok() {
            load_registry_dump(&dump_path).unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    if let Ok(mut cache) = CONTRACT_RECORDS_CACHE.lock() {
        *cache = Some((cache_key, records.clone()));
    }
    records
}

fn load_registry_dump(path: &Path) -> CordialResult<Vec<ContractRecordDump>> {
    let content = std::fs::read_to_string(path)?;
    let dump: RegistryDump = serde_json::from_str(&content)
        .map_err(|err| CordialError::json_parse(path.display().to_string(), err))?;
    Ok(dump.contract_records)
}

fn workspace_has_amenable(workspace_root: &Path) -> bool {
    cargo_metadata::MetadataCommand::new()
        .current_dir(workspace_root)
        .exec()
        .ok()
        .is_some_and(|meta| {
            meta.packages
                .iter()
                .any(|package| package.name == "amenable")
        })
}

fn run_amenable_dump_registry(workspace: &Path, out_path: &Path) -> CordialResult<()> {
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let status = Command::new("cargo")
        .current_dir(workspace)
        .arg("run")
        .arg("-p")
        .arg("amenable")
        .arg("--features")
        .arg(AMENABLE_DUMP_REGISTRY_FEATURES)
        .arg("--")
        .arg("dump-registry")
        .arg("--out")
        .arg(out_path)
        .status()
        .map_err(|err| {
            CordialError::invariant(format!("failed to run amenable dump-registry: {err}"))
        })?;

    if !status.success() {
        return Err(CordialError::invariant(format!(
            "amenable dump-registry exited with {status}"
        )));
    }
    if !out_path.is_file() {
        return Err(CordialError::invariant(format!(
            "amenable dump-registry did not write {}",
            out_path.display()
        )));
    }
    Ok(())
}
