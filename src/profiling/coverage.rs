//! Coverage evidence and declared boundaries; all allocation quantities remain Hotpath's.
use serde::{Deserialize, Serialize};

/// A boundary that must accompany a memory report, including unverified work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Boundary {
    /// Region from the measurement plan's inventory.
    pub region: String,
    /// Evidence from controlled fixtures; never a claim of full-program coverage.
    pub evidence: String,
    /// Remaining limits or work that cannot be measured by this allocator.
    pub unverified: String,
}

/// The inventory travels with every producer, so downstream readers cannot hide gaps.
pub fn boundaries() -> Vec<Boundary> {
    [
        ("entry and setup", "lifecycle fixture: process/test entry through teardown", "pre-entry work uses a lifetime heap baseline; legacy init_tracing starts late"),
        ("pipeline", "shared IR inventories every non-const function, closures and async blocks", "full pipeline cache-hit/recomputation coverage awaits target review"),
        ("dependency work", "named Salsa verification and worker-item scopes", "opaque dependency origins remain at their caller boundary; full query coverage unverified"),
        ("concurrent work", "controlled short-lived/exited threads, cross-thread frees and registry overflow", "no supported cross-spawn parent linkage; attribution requires zero overflow"),
        ("async work", "poll-aware async scope fixtures: yield, migration and cancellation", "escaping closures/async blocks require separate scopes; no synchronous guard may span await"),
        ("eviction and shutdown", "ordinary/error/unwind teardown captured before finish", "runtime workers, background drops and TLS destructors after finish remain unverified; controlled exits bypass stack teardown"),
        ("isolation", "resumed, missing, crashed and spawn-error child fixtures", "uncatchable exits retain incomplete manifests, not a fabricated partial report"),
        ("other subprocesses", "expected workflow children retain state/repeat and identity", "Cargo, rustc, proc-macro and external helpers have separate heaps; OS child envelope unavailable"),
        ("test processes", "early explicit owner and legacy late owner; missing expected tests visible", "test harness allocation outside test_entry is not attributed to that test"),
        ("recorder and native memory", "Hotpath exposes recorder/unscoped/TLS/overflow traffic and retained origins", "native allocations bypassing the allocator, mmap, stacks, shared pages and allocator retention require a separate OS envelope; heap subtraction is not an exact partition"),
    ].into_iter().map(|(region,evidence,unverified)| Boundary {region:region.into(),evidence:evidence.into(),unverified:unverified.into()}).collect()
}

/// Static coverage of one canonical declaration. This does not assert runtime execution.
#[cfg(feature = "tracing")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationCoverage {
    /// File-local declaration path.
    pub path: String,
    /// Source line for review.
    pub line: u32,
    /// Shared-loader evidence, absent for inventories from older producers.
    pub facts: Option<crate::ir::SourceHotpathFunction>,
}

/// Read shared IR facts only. Untraced functions and escaping bodies remain visible.
#[cfg(feature = "tracing")]
pub fn operation_coverage(ir: &dyn crate::IrView) -> crate::CordialResult<Vec<OperationCoverage>> {
    let mut rows = Vec::new();
    for node in ir.nodes_matching(
        &crate::QueryBuilder::new()
            .has_attr(crate::ATTR_SOURCE_TRACING_INVENTORY)
            .build(),
    ) {
        if let Some(inventory) = ir.source_tracing_inventory(node.id())? {
            rows.extend(
                inventory
                    .functions()
                    .iter()
                    .map(|function| OperationCoverage {
                        path: function.path().clone(),
                        line: function.line(),
                        facts: function.hotpath().clone(),
                    }),
            );
        }
    }
    Ok(rows)
}
