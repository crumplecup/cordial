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
//! - **Creusot**: `#[requires(...)]`/`#[ensures(...)]` attributes are
//!   selected from the shared Rust tree. Their Pearlite payloads use the
//!   database's memoized dialect query rather than a second whole-file parse.
//! - **Verus**: `requires`/`ensures` clauses inside `verus!` are opaque to
//!   the Rust tree. Their tokens use the same memoized dialect boundary.
//! - **Kani**: `assert!`, `assert_eq!` and `kani::assume` inside proof
//!   functions are collected from canonical syntax, including the shared
//!   embedded-program query for `amenable_derive::harness!`. Intermediate
//!   bindings require dataflow analysis and are not followed by this policy.
//!
//! Any `gallery` directory (structural convention, not a per-project
//! config option) is pruned from the walk entirely: it holds verifier
//! experiments and documented dead ends, not production proofs, so its
//! raw clauses are never candidates for a named contract type.
//!
//! A clause is accepted not by matching its *text* against a registered
//! fragment, but by recognizing its *shape* as a real call to a
//! registered contract's predicate — see [`ContractClause`]
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

use tracing::instrument;

use crate::error::CordialResult;
use crate::etiquettes::antipatterns::scan::truncate_snippet;
use crate::etiquettes::antipatterns::types::{AntipatternRuleId, AntipatternSiteRecord};
use crate::loader::SourceLoadView;

mod facts;
pub(crate) use facts::ContractClause;
mod index;
mod native;
pub(super) use native::collect_native;
pub(crate) use native::collect_payload;
mod registry;
mod verus;

pub(crate) use index::verifier_for_crate;
pub use index::{ContractRecordDump, ContractRecordDumpBuilder};
pub use registry::fetch_contract_records;

#[instrument(level = "debug", skip(clauses, registry))]
pub(super) fn assess_contracts(
    clauses: &[ContractClause],
    registry: &[ContractRecordDump],
) -> Vec<AntipatternSiteRecord> {
    let index = index::ContractIndex::build(registry);
    clauses
        .iter()
        .filter_map(|clause| clause.finding(&index))
        .collect()
}

#[instrument(level = "debug")]
pub(super) fn site_context(module_prefix: &[String], leaf: &str) -> String {
    let mut parts = module_prefix.to_vec();
    parts.push(leaf.to_string());
    parts.join("::")
}

#[instrument(level = "debug", skip(file))]
pub(super) fn make_finding(
    context: String,
    file: &Path,
    line: u32,
    normalized: &str,
) -> CordialResult<AntipatternSiteRecord> {
    AntipatternSiteRecord::builder()
        .rule_id(AntipatternRuleId::UnnamedContractBound001)
        .context(context)
        .file(file.to_path_buf())
        .line(line)
        .snippet(truncate_snippet(normalized, 96))
        .build()
}

/// Scan one Creusot source string (used by tests, mirroring
/// `scan_antipatterns_rust_source`'s shape for the other three rules).
#[instrument(level = "debug", skip(source, file, registry), err(level = "warn"))]
pub fn scan_creusot_contract_bounds_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    registry: &[ContractRecordDump],
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    SourceLoadView::collect_native_fixture(file, source, |source| {
        let index = index::ContractIndex::build(registry);
        Ok(native::collect_file(source, src_root, "creusot")?
            .iter()
            .filter_map(|clause| clause.finding(&index))
            .collect())
    })
}

/// Scan one Verus source string (used by tests).
#[instrument(level = "debug", skip(source, file, registry), err(level = "warn"))]
pub fn scan_verus_contract_bounds_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    registry: &[ContractRecordDump],
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    SourceLoadView::collect_native_fixture(file, source, |source| {
        let index = index::ContractIndex::build(registry);
        Ok(native::collect_file(source, src_root, "verus")?
            .iter()
            .filter_map(|clause| clause.finding(&index))
            .collect())
    })
}

/// Scan one Kani source string (used by tests).
#[instrument(level = "debug", skip(source, file, registry), err(level = "warn"))]
pub fn scan_kani_contract_bounds_source(
    source: &str,
    file: &Path,
    src_root: &Path,
    registry: &[ContractRecordDump],
) -> CordialResult<Vec<AntipatternSiteRecord>> {
    SourceLoadView::collect_native_fixture(file, source, |source| {
        let index = index::ContractIndex::build(registry);
        Ok(native::collect_file(source, src_root, "kani")?
            .iter()
            .filter_map(|clause| clause.finding(&index))
            .collect())
    })
}
