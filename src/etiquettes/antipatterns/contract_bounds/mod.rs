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

use std::path::Path;

use proc_macro2::{Delimiter, TokenTree};
use syn::{ItemFn, ItemMacro};
use tracing::instrument;
use walkdir::WalkDir;

use crate::error::CordialResult;
use crate::etiquettes::antipatterns::scan::truncate_snippet;
use crate::etiquettes::antipatterns::types::{AntipatternRuleId, AntipatternSiteRecord};

mod creusot;
mod index;
mod kani;
mod registry;
mod verus;

pub use index::ContractRecordDump;
use index::verifier_for_crate;
pub use registry::fetch_contract_records;

/// A `gallery` directory holds verifier experiments, not production
/// proofs (see e.g. `amenable_kani::gallery`'s own doc comment: "the
/// gallery answers a different question" than the production queue).
/// Its `assert!`/`#[ensures]`/`requires` clauses are deliberately
/// throwaway probes, not reusable bounds worth a named contract type,
/// so the whole subtree is pruned from this rule's walk rather than
/// flagged and then hand-excepted site by site.
#[instrument(level = "trace", skip(entry), ret)]
fn is_gallery_dir(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir() && entry.file_name() == "gallery"
}

/// Scan one crate's `src/**/*.rs` tree for the backend it's written for.
/// Returns no findings for any other crate — this rule only applies to
/// the three verifier crates.
#[instrument(level = "debug", skip(registry), err(level = "warn"))]
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

    let index = index::ContractIndex::build(registry);
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
            "creusot" => {
                creusot::scan_creusot_source(&source, path, &src_root, crate_name, &index)?
            }
            "verus" => verus::scan_verus_source(&source, path, &src_root, crate_name, &index)?,
            "kani" => kani::scan_kani_source(&source, path, &src_root, crate_name, &index)?,
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

#[instrument(level = "debug")]
pub(super) fn site_context(module_prefix: &[String], leaf: &str) -> String {
    let mut parts = module_prefix.to_vec();
    parts.push(leaf.to_string());
    parts.join("::")
}

#[instrument(level = "debug", skip(file))]
pub(super) fn make_finding(
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
#[instrument(level = "debug", skip(source, file, registry), err(level = "warn"))]
pub fn scan_creusot_contract_bounds_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    registry: &[ContractRecordDump],
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    creusot::scan_creusot_source(
        source,
        file,
        src_root,
        crate_name,
        &index::ContractIndex::build(registry),
    )
}

/// Scan one Verus source string (used by tests).
#[instrument(level = "debug", skip(source, file, registry), err(level = "warn"))]
pub fn scan_verus_contract_bounds_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    registry: &[ContractRecordDump],
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    verus::scan_verus_source(
        source,
        file,
        src_root,
        crate_name,
        &index::ContractIndex::build(registry),
    )
}

/// Scan one Kani source string (used by tests).
#[instrument(level = "debug", skip(source, file, registry), err(level = "warn"))]
pub fn scan_kani_contract_bounds_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    crate_name: &str,
    registry: &[ContractRecordDump],
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    kani::scan_kani_source(
        source,
        file,
        src_root,
        crate_name,
        &index::ContractIndex::build(registry),
    )
}

/// `amenable_derive::harness!(cfg_name, CONST_NAME, { item })` wraps nearly
/// every real Kani and Creusot proof in this workspace — `syn::parse_file`
/// sees the whole invocation as one opaque `Item::Macro`, the same blind
/// spot `verus!{}` has, so the wrapped `fn` is otherwise invisible to
/// `syn::visit::Visit`. Extracts and parses the trailing brace-delimited
/// group (`{ item }`) as a real `ItemFn`, so it can be fed back into the
/// same visitor logic that already handles top-level functions.
#[instrument(level = "debug", skip(node))]
pub(super) fn harness_macro_item_fn(node: &ItemMacro) -> Option<ItemFn> {
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
