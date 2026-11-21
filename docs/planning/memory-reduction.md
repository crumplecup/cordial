# Memory reduction after the first elicit_doc measurement

Status: **Implementation in progress, 2026-11-20.** Phase 1A is implemented;
its checkpoint evidence is below. Phases 1B–1E remain before optimization.
Phase 1 adds all currently identified measurement gaps before optimization
starts. The original [measurement infrastructure plan](memory-measurement.md)
is complete; this plan continues that work.

## Evidence and boundaries

The first successful cold measurement used Cordial `65cbde11` against
`elicit_doc` `8d2968a7`, with both memory controls set to zero. It completed in
168.49 seconds. The [analysis and linked raw evidence](/home/erik/.cordial/elicit_doc/perf/1795226257-8d2968a7-memory-analysis.md)
are retained in the existing project perf store. No named baseline was promoted.

The exact pipeline RSS high-water reading was 3,366,715,392 bytes (3.14 GiB).
The later Hotpath RSS maximum was rounded to approximately 3.2 GiB. Requested
heap peaked at 2,535,814,534 bytes (2.36 GiB); allocation traffic was
20,365,878,282 bytes (18.97 GiB). These are different quantities.

| Measured allocation origin | Individual peak, MiB | Design opportunity |
| --- | ---: | --- |
| Definition-map construction | 707.06 | Identify disposable dependency caches and their last consumers. |
| Hotpath recorder | 676.08 | Remove large fixed histogram/log capacity from exact-summary capture. |
| IR snapshot | 463.01 | Serialize the graph without a deep ownership copy. |
| Semantic body traversal | 384.69 | Separate expansion, resolution, inference and summary storage. |
| SavedState::save | 213.89 | Avoid simultaneous flattened and serialized dependency representations. |
| Projection::populate | 205.58 | Share immutable facts between memoized items and the graph. |
| SourceDatabase::project_attached | 141.14 | Measure policy/context copying before changing ownership. |
| workspace::load_database | 128.78 | Identify retained inputs and transient workspace state. |
| Unscoped | 128.51 | Add useful operation boundaries and preserve residual counters. |
| items::resolve_flows | 127.89 | Remove JSON clones and typed/JSON round trips. |
| Optional evidence decoding | 71.22 | Reuse borrowed or shared facts across consumers. |
| Copy-context function discovery | 48.52 | Share context-invariant facts without duplicating existing memoization. |

These peaks occurred independently and **must not be added**. The existing
capture cannot partition ownership at its global heap peak. Recorder histogram
arrays are estimated from the code at about 618.22 MiB across 1,084 rows; this
is a hypothesis to quantify in phase 1, not another measured heap category.

## Execution contract

- Hotpath remains the sole allocation counter and attribution authority. Extend
  it where necessary; do not add a second ledger, allocator, pointer registry,
  span-delta collector, or shell measurement program.
- Use existing `PerfStore` records, linked raw captures and reusable work state.
  Do not create another storage root, manual run directory, or baseline scheme.
  Preserve the first capture and record explicit baseline selection.
- Develop against small controlled tests under `tests/`. Do not execute Cordial
  against itself, including `open`, `submit`, or `quality`. The maintainer's
  instruction overrides the usual self-analysis commit gate for this effort.
- At every implementation checkpoint, run focused tests, `cargo fmt --check`,
  `cargo clippy --all-features --all-targets -- -D warnings`, and
  `git diff --check`; then commit the completed checkpoint. A failed gate blocks
  that commit. Update this plan with evidence and the commit id.
- Use allocation bytes from Hotpath for memory regression assertions. Isolate
  allocator-sensitive tests in subprocess fixtures; use bounded inputs and
  relative growth checks rather than timing benchmarks or exact RSS assertions.
  Existing JSON-size estimates are not substitutes for measured heap bytes.
- Preserve results, cfg/macro completeness, graph identities, apply behavior,
  persistence, cache invalidation and budget enforcement. Query counts and
  encoded output sizes complement allocation data; neither measures heap.
- A phase is complete only when its acceptance checks pass. Conditional work
  must receive a recorded evidence-based disposition, rather than disappear.
  Do not claim a reduction in whole-program peak from an individual origin.

## Phase 1 — Close the current measurement gaps

Complete every checkpoint in this phase before changing application storage or
cache algorithms. Instrument the existing dependency paths rather than building
another reconstruction of source facts.

### 1A. Simultaneous ownership at the global requested-heap peak

1. Extend `vendor/hotpath/src/retained.rs` to retain origin live bytes and block
   counts at the allocator-lifetime global peak. Include reserved origins,
   accounting interval, baseline, event sequence and tie policy. Keep capture-end
   ownership and individual origin peaks as separate fields.
2. Update the peak composition inside the registry's existing critical section,
   so its components and total describe the same linearized allocator event.
   Use bounded preallocated metadata and changed-origin tracking; avoid copying
   the full origin table on every new high-water event. No allocator calls,
   formatting, TLS callbacks or user callbacks inside the registry lock.
3. Export exact integers through Hotpath JSON, the existing importer, run record,
   renderer and comparator. Describe allocation origin, not logical ownership.
   Keep the cheap non-retained live meter free of this registry work.

Acceptance: extend `hotpath_retained`, `hotpath_alloc_contract` and `perf_memory`
with disjoint allocation waves whose individual peaks cannot be summed,
concurrent frees/reallocations, failure, ties and overflow. At a complete peak,
all origin components sum to its exact total; unavailable composition never
falls back to ending ownership. Round-trip integers above 2^53. Verify bounded
metadata and changed-origin update work on a small many-origin fixture.

### 1B. Exact OS envelope and interval alignment

1. Export exact bytes from Hotpath's existing RSS observation rather than its
   formatted string. Import the existing `src/process_memory.rs` phase samples
   and final high-water observation through the same perf record.
2. Record process identity, observation time/phase, platform, units and whether
   each value is current RSS, sampled maximum, or OS lifetime high-water.
   Collect available anonymous/file/shared envelope details through the existing
   OS sampler; unsupported platforms or unavailable reads remain explicit.
3. Align these observations with capture and pipeline boundaries. An OS lifetime
   maximum has no inferred timestamp, and an RSS sample is not a heap snapshot.
   At existing major phase boundaries, persist Hotpath's current live-origin
   snapshot with the phase identity. This reveals retention across phases in
   addition to the composition at the single global heap peak; it does not
   assign ownership from before/after subtraction.

Acceptance: parser/import fixtures cover exact large values, unavailable reads,
rounded legacy input and different intervals. Never derive native ownership by
subtracting requested heap from RSS. No exact RSS assertion in a unit fixture.

### 1C. Attribute dependency, copying and recorder work

Add Hotpath scopes at the following actual allocation boundaries, maintaining
an inventory of the label, source location, lifetime and fixture that proves it:

| Area | Required distinction |
| --- | --- |
| Hotpath recorder | Function registration/histogram capacity, recent logs, queued work, report aggregation and export copies. |
| Definition maps/body queries | Parsing, expansion, definition-map construction, resolution/inference, compact result construction and cache retention. |
| Salsa persistence | Dependency flattening, shared-set construction, origin conversion, memo serialization and encoding buffers. |
| IR persistence/projection | Snapshot creation, attribute clone/decode, population, encoding, policy attachment and context overlay copying. |
| Flow/evidence/context discovery | Typed fact construction, JSON conversion, call resolution, evidence decoding and context-invariant versus cfg-dependent work. |
| Workspace/process lifetime | Workspace load, worker startup, background writes, completion, teardown and unscoped work. |

Use dependency-local scopes where enclosing Cordial labels are too coarse.
Route instrumentation of the recorder through its reserved recorder origin;
avoid recursively recording its own recording activity. Preserve separate
exclusive/inclusive accounting. Use existing query counters to expose cache
hits, recomputation and the effect of later eviction changes.

Acceptance: extend `hotpath_coverage`, `hotpath_workers`, `hotpath_queries` and
small dependency fixtures to exercise every new boundary. Explain remaining
unscoped and hidden/pre-thread traffic in the coverage inventory; retain exact
residual counters rather than relabeling all unknown work as measured.

### 1D. Process, worker and teardown coverage

1. Audit existing manifest expectations against every actual spawn/resume and
   external-helper path. Preserve parent/child identity, start/exit status,
   missing reports and PID reuse distinctions. Use existing process sampling
   for an external child's envelope where observable.
2. Verify workers, async polls, background cache writes and recorder drain/join
   finish before the final relevant snapshot. Identify allocations/frees after
   capture closure, TLS destructor limitations and uncatchable exits explicitly.
3. Update the existing ten-region coverage inventory with tested evidence.
   Native allocations, mmap, stacks, allocator retention and uninstrumented
   helper heaps remain explicitly outside requested-heap ownership. Short-lived
   children can escape sampling; sums of sampled RSS are not unique physical
   memory or an exact process-tree peak.

Acceptance: extend `hotpath_lifecycle`, `hotpath_workers` and
`perf_workflow_partial` using controlled children and delayed worker/drop cases.
Tests distinguish complete, unsupported, missing, malformed and truncated data.
Do not mark an opaque or native boundary complete merely because a caller has
a scope. Record what would require separate native/dependency instrumentation
and discussion instead of introducing another measurement system.

### 1E. Measurement checkpoint and comparison contract

Version new producer fields, modes and accounting metadata; update the vendored
patch/provenance alongside source changes. Extend `perf_record`, `perf_compare`
and `perf_memory` for older captures, partial evidence and incompatible modes.
Render a simultaneous peak table and separate OS envelope/coverage table.

Phase exit: all additions above are unit-proven and gated/committed. Then make
an unlimited cold `elicit_doc` capture through the existing workflow. Use it to
resolve recorder components and candidate cache lifetimes before optimization.
Keep the original capture: if schemas/accounting differ, establish a new
compatible comparison baseline and explain the bridge. Do not fabricate exact
deltas against legacy evidence or silently promote a baseline.

## Phase 2 — Reduce the memory cost of measuring memory

1. In `vendor/hotpath/src/lib_on/functions/alloc/state.rs`, add an explicit
   exact-summary capture mode without percentile histograms or recent-call
   storage. Preserve exact totals, timing totals, retained origins, peak
   composition and coverage. Report unavailable percentiles as unavailable.
2. For the percentile mode, lazily allocate histogram ranges and grow them with
   observations instead of eagerly reserving the maximum range per function.
   Preserve precision, clamping and aggregation semantics. Keep mode selection
   explicit in raw evidence and comparisons.
3. Use phase-1 evidence to remove expensive recorder/export clones and bound
   queued temporary state if those contribute materially. Drain/backpressure
   must preserve events; do not solve recorder overhead with lossy sampling.

Acceptance: controlled many-function fixtures compare exact counters/origins
across modes and check percentile behavior, zero observations, late registration
and range growth. Check summary storage scales with compact rows rather than
three maximum-sized histograms per row; add a conservative measured per-row
allocation bound. Prove export does not duplicate histogram-sized state where
that change is needed. Gate and commit summary and histogram changes separately.

## Phase 3 — Remove the graph-sized IR persistence copy

1. Introduce a borrowed serializable view of `CrateIr` in `src/ir/graph.rs`, using
   the current node/edge/index wire shape. Route cache/stage serialization through
   it instead of `snapshot()` deep-cloning the graph.
2. Change `src/run_cache/store.rs` to write while borrowing the graph. Initially
   accept synchronous cache writing to remove the second representation; record
   the throughput tradeoff. Keep owned snapshots where ownership is actually
   required, with a common wire implementation.
3. Preserve buffered compression, partial-file cleanup, atomic rename, error
   reporting and completion semantics. Encoding already streams; adding another
   streaming buffer alone does not remove this copy.

Acceptance: compare decoded old/new wire data, node/edge/index identities and
cache round trips; exercise failed/interrupted writes and absence of committed
partial caches. A controlled graph with large attributes must serialize without
a graph-sized allocation increase. Gate and commit before changing attributes.

## Phase 4 — Share facts and eliminate representation round trips

### 4A. Immutable attribute storage

`src/ir/node.rs` already packs bulk function attributes. Share immutable packed
bytes instead of cloning their allocation between memoized `FileItems`, graph
nodes and snapshots. Give clones an empty decoded cache rather than copying a
populated `OnceLock`; keep decoded-cache lifetimes independently reclaimable.
Preserve mutation isolation through replacement or copy-on-write semantics.

Acceptance: extend `ir_attribute_packing` and allocation fixtures for unread,
decoded, cloned, mutated and dropped attributes. Preserve repeated-key/latest
lookup behavior and JSON wire data. Cloning an unchanged bulk payload must not
allocate another payload-sized buffer; dropping decoded caches must reclaim them.

### 4B. Typed collection through flow resolution

First remove intermediate JSON clones in `src/loader/database/items.rs` by
deserializing from borrowed values and borrowing target syntax where possible.
Then carry typed facts through collection and flow resolution, encoding once at
the IR boundary. Adapt `populate.rs` to reuse immutable facts rather than clone
every item/module payload. Keep one authoritative fact path through the shared
loader and `IrView`; do not add a parallel source scan or unbounded side cache.

### 4C. Evidence and projection consumers

The optional evidence reader already deserializes from borrowed JSON, but its
results are owned. Add scoped borrowing or immutable typed access where repeated
consumers allocate the same facts. Address policy/context projection copies
identified in phase 1 by borrowing inputs and releasing finalized accumulators.
Retain independent context-dependent results where required.

Acceptance for 4B/4C: semantic and flow graph equality, context overlays,
source-call trust and apply/evidence behavior remain unchanged. Controlled
multi-consumer fixtures show one retained bulk representation and reduced
conversion traffic as consumer count grows. Existing `semantic_recording`,
`semantic_callees`, `semantic_backend` and context fixtures provide correctness
coverage. Gate and commit 4A, 4B and 4C separately.

## Phase 5 — Bound Salsa persistence temporaries

1. In `vendor/salsa/src/function.rs`, consume or borrow owned edges during
   conversion from flattened dependencies to serialized origins. Remove the
   overlap of cloned origin edges with the retained pending/flattened list.
2. Use a serialization adapter or bounded two-pass traversal to avoid retaining
   both full origin representations. Preserve existing shared dependency sets;
   do not expand deduplicated sets back into repeated edge vectors.
3. Bound pending keys and encoding temporaries where phase-1 evidence supports
   it. `SavedState::save` already streams the main database; its sub-MiB side
   files are not a major RAM target. Avoid a separate persistence implementation.
4. Preserve the wire format where feasible; otherwise version and test explicit
   cache invalidation. Synchronize vendor patch/provenance changes.

Acceptance: `salsa_flatten`, `saved_state`, `saved_contexts`, `saved_generated`,
`saved_proc_macros`, `salsa_tracked_identity`, `salsa_lazy` and `salsa_spill`.
Add diamond/high-fanout dependency fixtures, shared sets and direct/transitive
edits across cfg contexts. Saved/reloaded results and invalidation must agree
with fresh execution. Hotpath must show reduced temporary bytes with increasing
edge count, without retaining a second complete origin representation.
Gate and commit the edge-copy removal before a larger serialization change.

## Phase 6 — Make semantic working sets follow useful lifetimes

### 6A. Definition-map, expansion and inference caches

1. Use phase-1 scopes and query counters to make a table of cache classes, bytes,
   producers, ordinary/cfg/observability/apply/persistence consumers, last-use
   barriers and reload costs. Select actual disposable classes from this table.
2. Retain compact semantic results and reusable definition maps while releasing
   temporary parse/expansion/inference scaffolding after its last consumer,
   including unlimited-budget runs. Preserve the existing needed-crate closure
   and dependency order; workspace prefill is already disabled.
3. Make release compatible with persistence/spill and later readers. A pressure
   threshold change alone is not an algorithmic win. Do not omit functions,
   macros, cfg contexts or evidence to make a smaller report.

### 6B. Context-invariant discovery and workspace inputs

Copy-context discovery is already a persisted query returning borrowed memoized
results. Identify repeated identity/input facts and share only those invariant
under source, cfg, edition, macros and dependencies. Keep context-sensitive
resolution separate. Release completed context/workspace intermediates at a
proven last-use boundary; avoid another memoization layer or an immortal pool.

Acceptance: `semantic_workspace`, `semantic_backend`, `gated_semantics_demand`,
`cfg_contexts`, `context_reuse`, `context_parses`, `context_crates` and
`saved_contexts`, with transitive dependencies and macro/config changes.
Compare results with fresh computation under zero and bounded budgets. Assert
unchanged warm requests do not cause extra recomputation, necessary edits still
invalidate, and retained temporary state follows the active working set rather
than all completed contexts. Document any cache that cannot safely be released.
Gate and commit each independently justified cache/context change.

## Phase 7 — Verify the combined result and close the plan

1. Run the focused suites for the combined changes and the common all-features
   Clippy gate. Include disabled profiling, timing-only and alloc-enabled
   lifecycle/import checks so normal budget behavior remains available.
2. Measure `elicit_doc` cold with both controls zero, through the same existing
   workflow/store and release feature set. Keep the target revision fixed or
   explain why a fresh matched baseline is necessary. Record repeated captures
   when needed to distinguish timing/RSS variation; no Cordial self-benchmark.
3. Compare compatible captures for exact heap peak/composition, OS high-water,
   recorder overhead, traffic, saved-state size, query counts and elapsed time.
   Verify semantic output equivalence and that work/coverage did not disappear.
   Separate application improvement from profiler-mode changes.
4. Publish the operation-by-operation breakdown and residual coverage limits in
   the existing perf store, linked here. Record which proposed wins shipped,
   which were disproved, and remaining costly cache classes. Keep historical
   captures immutable and baseline changes explicit.

Completion means demonstrated reductions and preserved behavior, not an assumed
sum of local savings or a promised 1.5 GB target. Any decision to restore a
budget or begin Cordial self-analysis requires a separate discussion after
these results are available.

## Implementation checkpoints

### Phase 1A — Simultaneous global heap peak composition

Hotpath now retains exclusive origin bytes and block counts at the first
attainment of its allocator-lifetime requested-heap peak. The existing registry
lock linearizes the total, composition and successful-event sequence. Fixed
metadata and a dirty-origin list bound copying work by successful allocator
events; the non-retained meter keeps its two existing atomic counters.

The exact peak survives JSON, import and immutable perf-record storage. The
renderer separates it from ending origins and individual origin peaks. Missing
legacy composition remains unavailable; inconsistent or unsupported composition
is retained as raw evidence and withheld from exact breakdowns/comparisons.
The producer identifies the extension as `0.31.2+cordial-retained-peak-v1`.

Controlled evidence:

- All-features tests: `hotpath_alloc_contract` (2), `hotpath_heap` (6),
  `hotpath_retained` (11), `perf_compare` (6), `perf_memory` (12),
  `perf_record` (9), `hotpath_json` (1), `hotpath_lifecycle` (7): 54 passed.
- Disabled profiling: `cargo test --locked --offline --no-default-features
  --test hotpath_heap`: 6 passed. Timing-only: add `--features hotpath` and
  `--test hotpath_json --test hotpath_lifecycle`: 15 passed.
- Fixtures cover disjoint waves, equal byte peaks with different block counts,
  intervening frees, realloc/failure, concurrent churn, reserved/overflow
  origins, a pre-capture peak, integers above 2^53, legacy reports and malformed
  or unsupported evidence. Metadata stays fixed, and origin-copy counts remain
  bounded by successful events.
- The synchronized vendored patch reconstructs all 108 source files exactly
  from the pinned published archive. Provenance and the contract were updated.

Gate: `cargo clippy --locked --offline --all-features --all-targets -- -D
warnings`, `cargo fmt --check`, the vendored-source `rustfmt --check`, and
`git diff --check` passed. This evidence is committed with the checkpoint;
its commit id can be resolved with `git log -- docs/planning/memory-reduction.md`.
Phase 1B is next; no application memory optimization or further target
measurement has started.
