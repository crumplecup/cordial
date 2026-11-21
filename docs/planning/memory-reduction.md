# Memory measurement and reduction using upstream Hotpath

Status: **Upstream restoration, persistence-copy removal and phase 4 steps 1–2 complete; step 3 in progress, 2026-11-21.**
The immediate checkpoints restored published Hotpath with focused consumer
tests and removed the graph-sized serialization copy. Existing measurement
gaps are accepted and documented; they do not block application improvements.
Later instrumentation items remain optional follow-ups where they answer a
specific optimization question. No further full-program run is authorized here.

## Contract

- Depend on the published Hotpath crate unchanged. Remove `vendor/hotpath`,
  its patch/provenance machinery, and the Cargo override. No local copy, fork,
  patched dependency, custom allocator implementation, retained-pointer registry,
  allocation ledger, or span-delta profiler.
- Cordial owns operation labels, process lifecycle, coverage descriptions, and
  import/storage/presentation. Hotpath owns allocation collection. Existing OS
  readings remain the process envelope and operational pressure input; they
  never become an attribution system.
- Complete measurement infrastructure and controlled tests before application
  optimization. No Cordial self-execution, including `open`, `submit`, `quality`,
  or benchmarks. Discuss readiness before another full-program measurement.
- Use the existing `PerfStore`, linked raw reports and reusable workflow state.
  No new measurement root, per-manual-run directory scheme, shell measurement
  program, or second baseline system. Preserve historical captures unchanged;
  baseline selection is explicit.
- Each implementation checkpoint requires focused tests under `tests/`,
  `cargo fmt --check`,
  `cargo clippy --all-features --all-targets -- -D warnings`, and
  `git diff --check`, followed by a commit. A failed gate blocks the commit.
  The maintainer's no-self-execution instruction replaces the usual Cordial
  submit gate for this work.
- Preserve semantic results, cfg/macros, graph identity, apply behavior,
  persistence/invalidation, worker reuse and resumable execution. Do not change
  cache algorithms while repairing the measurement integration.

## Verified capabilities and limits

The published `hotpath-0.31.2` archive, rather than the modified local copy, was
inspected for this revision. Use that published version for the migration to
avoid combining removal with an unrelated version upgrade. Record the actual
resolved crate version and features in every capture.

Upstream supports `hotpath-alloc`, `CountingAllocator`,
`#[hotpath::measure]`, `hotpath::measure_block!`, `HotpathGuardBuilder`, JSON
reports, thread allocation/deallocation totals and process RSS reporting.
See upstream [function profiling](https://hotpath.rs/functions),
[guard/report lifecycle](https://hotpath.rs/profiling_modes),
[configuration](https://hotpath.rs/configuration), and
[thread metrics](https://hotpath.rs/threads).

| Question | Supported evidence and limits |
| --- | --- |
| Which operations allocate heavily? | Instrumented function/block allocation traffic. Default exclusive attribution; cumulative results overlap and are unsuitable for accounting, especially recursion. |
| Which threads allocate/free? | Upstream thread totals. Frees belong to the freeing thread; a thread difference does not identify an object's allocation origin or owner. |
| How much process RAM was observed? | Upstream RSS/thread reports and existing OS observations. Distinguish sampled current RSS from OS lifetime high-water, with their actual intervals and platform support. |
| What remains live by operation, and what composes the simultaneous heap peak? | Not supplied by the inspected public upstream API. Remove our implementation and mark these fields unavailable for upstream captures. Allocation traffic and per-call maxima are not retained heap. |
| Are values exact? | Ordinary JSON contains formatted strings for several allocation and RSS fields. Preserve the original values and precision; rounded values stay approximate. Do not manufacture exact integers. |
| Is every boundary covered? | Prove instrumented paths with fixtures; list opaque dependency interiors, unscoped work, native allocations, mmap, stacks, profiler internals, startup/shutdown and missing child reports explicitly. No claim of complete retained-heap accounting. |

Do not add an upstream feature solely to obtain a richer export without first
checking its local output, overhead and transport behavior. No cloud upload is
needed. A missing capability becomes a documented limitation and, if useful,
an upstream proposal for discussion. It is not permission to implement a fork.

## Checkpoint evidence — upstream restoration

Checkpoint 1A is complete. Cargo resolves the unchanged registry release 0.31.2;
the local Hotpath source and patch are removed. Consumers preserve upstream
formatted traffic/RSS and historical raw evidence, withholding incompatible
comparisons. Budget pressure uses RSS, including allocator-retained pages.

Validation passed: allocation-enabled lifecycle/worker/coverage/import/precision
fixtures; disabled lifecycle/import fixtures; timing-only JSON; no-default library
check; perf record/comparison tests; 25 injected pressure tests and four Salsa
eviction/persistence fixtures. The eviction fixtures now serialize access to
their shared counters after concurrent execution exposed interference.
All-features/all-targets Clippy with warnings denied, formatting and diff checks
passed. Three memory-limit CLI workload tests were deliberately excluded; no
Cordial workload or full-program measurement was executed. Unsupported async,
retained ownership and dependency-interior coverage remains explicit.

## Checkpoint evidence — borrowed IR persistence

Phase 3 is complete. File and stage encoding serialize borrowed nodes, edges and
indexes in the existing snapshot wire format. Owned snapshots remain available
when ownership is needed. Packed attributes decode one cold value temporarily;
serialization neither fills nor clears the live graph's decoded caches.

The run cache encodes synchronously and publishes a same-directory temporary
file atomically after success. This removes the background writer's deep graph
copy and pending-writer bookkeeping. Failed publication removes the temporary
file and preserves the prior destination. Normal error/unwind cleanup does not
guarantee cleanup after an uncatchable process kill; an unpublished temporary
file may remain. Encoding no longer overlaps later pipeline stages, so the
whole-program throughput tradeoff remains unmeasured.

Validation passed: six attribute-packing fixtures and five persistence fixtures,
including file/stage wire equality, graph IDs and indexes, MessagePack, cold and
warm attribute-cache state, atomic replacement and publication failures.
Published Hotpath's cumulative allocation report on the same 4 MiB payload
reported approximately **8.1 MB owned versus 4.0 MB borrowed**. This demonstrates
the removed payload copy's allocation traffic, not a measured whole-program RSS
or peak reduction. All-features/all-targets Clippy with warnings denied,
formatting and diff checks passed. No Cordial or full-program workload ran.

Optional instrumentation checkpoints 1B–1D and phase 2 remain as described
below. There is no new full-program measurement authorization.

## Checkpoint evidence — shared packed attributes

Phase 4 step 1 is complete. Memoized node weights and their graph clones share
immutable packed bytes through `Arc<[u8]>`. A clone starts with an empty,
independent decoded cache, even when the source cache is warm. Replacement and
append continue to change only the destination node. No database, cfg-context
identity or persistence schema is shared or changed by this step.

The same published-Hotpath cumulative fixture ran before and after the change:

| Payload and clone count | Before | After |
| --- | ---: | ---: |
| 16 KiB, four cold clones | 64.8 KB | 804 B |
| 256 KiB, four cold clones | 1.0 MB | 804 B |
| 256 KiB, sixteen cold clones | 4.0 MB | 3.1 KB |
| 256 KiB, sixteen clones of a warm source | 8.0 MB | 3.1 KB |

These are upstream-formatted allocation-traffic values, not whole-process peak
RAM. Node containers and attribute keys still allocate per clone; initial packing
and each consumer's actual decoding still allocate. The fixture checks growth
with payload size, clone count and cache warmth without a custom allocator.

All 27 focused fixtures passed: attribute packing/sharing, IR persistence,
semantic backend/callees/recording, cfg-context isolation/invalidation and apply
snapshot consumption. All-features/all-targets Clippy with warnings denied,
formatting and diff checks passed. No Cordial workload ran.

## Checkpoint evidence — typed flow resolution

Phase 4 step 2 is complete. File item collection retains typed syntax and
semantic facts until flow resolution finishes, borrows the target's syntax for
each call, then encodes each node attribute once. The transient typed facts are
dropped after encoding; they are not another persistent fact cache. Attribute
order, graph identities and the separate cfg databases are preserved.

The same three-function fixture, with 32 calls in the target and 64 calls to
that target, ran before and after through published Hotpath:

| Operation | Before | After |
| --- | ---: | ---: |
| Flow resolution | 932.6 KB | 11.9 KB |
| File item collection, including flow resolution | 2.3 MB | 1.4 MB |

These are rounded cumulative allocation-traffic values. The rows overlap and
must not be added; peak RAM and whole-program throughput remain unmeasured.
The fixture verifies call identities, ranges, source digests and attribute order,
and rejects a return to the previous payload-copying allocation cost.

Validation: 33 focused fixtures cover attribute packing/sharing, persistence,
typed conversion, semantic backend/callees/recording, cfg-context isolation and
invalidation, conservative call-resolution contracts, and apply consumption.
All-features/all-targets Clippy with warnings denied, formatting and diff checks
passed. No Cordial workload ran.

## Checkpoint evidence — direct typed evidence reads

The first part of phase 4 step 3 reads cold packed semantic evidence directly
into its typed consumer, avoiding an intermediate expanded JSON tree. Presence
queries and context-activity checks inspect keys without decoding values. Saved
graph identity collection and remapping use the same typed path instead of
cloning JSON. Warm raw-JSON caches retain their existing behavior; incompatible
typed reads fall back to the JSON validation path with the existing error cause.
The encoding and persistence wire format are unchanged.

The same 256 KiB semantic-function fixture ran before and after through the
unchanged published Hotpath, with construction outside the capture:

| Operation | Before | After |
| --- | ---: | ---: |
| One typed evidence read | 515.5 KB | 256.2 KB |
| Four consumer stages, dropping decoded caches between stages | 2.0 MB | 1.0 MB |

These are rounded cumulative allocation-traffic values, not retained heap or
whole-program RSS. Each consumer still allocates its actual typed result. The
fixture also checks cold/warm cache state, wire round trips, latest repeated
keys, missing evidence, JSON null presence, distinct cfg contexts and malformed
evidence errors.

Validation: all 40 focused fixtures passed, including saved-context identity
remapping, semantic backend/callees/recording, context isolation/invalidation,
call-resolution contracts and apply consumption. All-features/all-targets
Clippy with warnings denied, formatting and diff checks passed. No Cordial or
full-program workload ran.

Phase 4 step 3 remains open for repeated whole-policy-inventory decoding and
remaining context copies. Source-call identity lookup now uses the field-only
reader described below when cfg-only functions require resolution.
Phases 5 onward remain planned. No full-program measurement is authorized.

## Checkpoint evidence — packed policy inventories

Phase 4 step 3 now also routes the loader's whole-crate policy inventory through
the existing packed attribute storage. Graph clones share its immutable bytes;
typed policy reads leave the expanded JSON cache cold. This adds no typed cache
or alternative collection path. Raw reads and persistence preserve unknown
fields, including nested fields, and malformed typed reads retain their JSON
error cause. The ten policy consumers still decode separate temporary typed
inventories; that repeated work remains open.

The same 256 KiB inventory fixture ran before and after through unchanged
published Hotpath:

| Operation | Before | After |
| --- | ---: | ---: |
| Four graph clones | 1.4 MB | 2.8 KB |
| Ten policy reads | 2.5 MB | 2.5 MB |
| Inventory construction plus ten reads | 2.5 MB | 3.3 MB |

These are rounded cumulative allocation-traffic values, not retained heap or
whole-program RSS. Packing adds one-time construction traffic in exchange for
removing expanded inventory copies from graph clones. It does not reduce the
ten consumers' typed-result allocations. Unknown-field/wire preservation,
clone/cache and mutation isolation, repeated-key lookup, malformed evidence and
feature combinations are checked with small fixtures.

Validation: 39 focused all-features checks and two allows-only checks passed,
covering packing/sharing, typed evidence, persistence, semantic invalidation,
apply consumption, policy findings, inactive sources and target identity.
All-features/all-targets Clippy with warnings denied, formatting and diff checks
passed. No Cordial CLI or full-program workload ran.

## Checkpoint evidence — field-only packed reads

Phase 4 step 3 now reads an ordinary function's definition ID directly from its
packed object during cfg-only call resolution. The existing attribute storage
skips unrelated encoded values through Serde and a borrowed MessagePack reader.
It copies only the selected field and leaves the full JSON cache cold. There is
no new fact cache, source scan or database sharing. Warm and unpacked lookups,
missing fields, non-object values and latest-key precedence retain JSON lookup
behavior; unknown fields and persistence wire values are preserved.

The same fixture ran before and after through unchanged published Hotpath:

| Lookup | Before | After |
| --- | ---: | ---: |
| Definition ID, 16 KiB unrelated payload | 17.3 KB | 16 B |
| Definition ID, 256 KiB unrelated payload | 257.3 KB | 16 B |
| Sixteen reads, 256 KiB unrelated payload | 257.3 KB | 256 B |

These are upstream-formatted cumulative allocation-traffic values. Previously,
the first read retained an expanded copy of the entire object in the JSON
cache; subsequent reads reused it. Field-only reads no longer create that
representation. Whole-program RSS and CPU tradeoffs remain unmeasured; each
field lookup still traverses the encoded object. Repeated policy decoding and
the remaining context copies are separate follow-ups within step 3.

Validation: 36 focused fixtures passed, covering field lookup, packing, typed
evidence, persistence, cfg isolation, conservative call resolution and apply
consumption. The cfg-only caller retains its context-specific target edge and
its opaque-namespace transformation refusal. All-features/all-targets Clippy
with warnings denied, formatting and diff checks passed. No Cordial CLI or
full-program workload ran.

## Phase 1 — Restore upstream use and finish measurement infrastructure

Checkpoint 1A restores a usable integration. Complete it before application
changes; pursue 1B–1D only when a concrete optimization needs their evidence.

### 1A. Remove the fork and its dependencies

1. Inventory all custom features, APIs, schemas and tests against the published
   archive. Include committed extensions and the uncommitted phase-1B work;
   deleting the vendor directory alone is insufficient.
2. Remove the Hotpath Cargo patch, local source and patch/provenance artifacts.
   Map Cordial's allocation feature to upstream `hotpath/hotpath-alloc`; remove
   `hotpath-live`, `hotpath-retained`, heap providers/snapshots, retained origins,
   peak composition and custom output-pinning hooks. Retain useful standard
   macros and the existing integration/lifecycle structure.
3. Use upstream `CountingAllocator` only for allocation-enabled builds, with one
   allocator declaration per executable. Timing-only and disabled builds must
   work without the fork's always-on heap meter.
4. Remove the budget dependency on custom `live_bytes()`. The proposed replacement
   is the existing RSS pressure path, explicitly named and tested as RSS. This
   changes the held-heap signal: freed objects may leave resident pages behind.
   Review that behavior before accepting this checkpoint; do not silently turn
   budgets off, use traffic totals as live memory, or add another counter.
5. Replace fork-only tests with consumer integration tests. Keep legacy capture
   reading where needed, labeled with producer provenance; do not translate old
   retained fields into upstream metrics. Correct active docs that claim the
   fork's capabilities belong to upstream.

Checks: Cargo metadata/lockfile resolve the registry crate with no Hotpath path
or git override; no custom API references remain. Controlled fixtures build and
run with profiling disabled, timing-only, and allocation-enabled. Test zero and
bounded pressure decisions with injected OS observations and verify eviction,
spill and resume behavior without executing Cordial on its own repository.
Gate and commit this migration separately.

### 1B. Instrument actual operations through upstream macros

Maintain an inventory of label, source boundary, allocating thread/task,
measurement scope and proving fixture. Prefer stable operation-sized labels;
avoid a new row for every input or tiny helper.

| Area | Cordial boundaries to distinguish |
| --- | --- |
| Semantic work | Workspace loading, dependency/definition-map requests, body queries, expansion/resolution requests and compact fact construction. |
| Persistence | IR snapshot/serialization, Salsa save/flatten calls, encoding and background write completion. |
| Facts and projection | Attribute clone/decode, projection population, typed fact construction, JSON conversion and flow resolution. |
| Consumers and contexts | Evidence decoding, policy attachment, context overlay copying and copy-context discovery. |
| Lifetime | Worker entry, asynchronous work, child/resumed process entry, completion and teardown within the capture lifetime. |

Place scopes in Cordial-controlled code. A caller around a dependency measures
that call's traffic on the measured execution path; it does not reveal internal
suboperations or allocations performed on other threads. Instrument worker
bodies independently. Do not alter dependencies to obtain finer labels under
this plan. Keep opaque interiors visible in the inventory.

Checks: nested exclusive scopes, recursion, cross-thread allocation/free,
worker execution, asynchronous polls/migration and background writes in small
subprocess fixtures. Establish upstream behavior rather than asserting old
fork semantics. Use known payloads and relative growth assertions at multiple
input sizes; report precision limits in tolerances. Query counts explain work
and recomputation, not heap ownership. Gate and commit by operation family.

### 1C. Use the supported report lifecycle and existing perf pipeline

1. Keep one upstream guard per process. Start before measured work and close
   after joined workers/background writes and relevant teardown. Use supported
   builder options for JSON, explicit sections and unlimited function/thread
   rows; account for upstream environment overrides rather than patching them.
2. Preserve parent/child identity and expected-report manifests. Every process
   needs its own report; missing, failed or truncated output is explicit. An
   opaque external helper does not acquire allocation attribution from its
   parent's scope.
3. Import upstream function metrics, thread totals, RSS and completeness metadata
   into the existing perf records. Record version, features, metric, attribution
   mode, effective limits, workload and capture interval. Distinguish unavailable,
   unsupported, approximate and zero. Do not require fork-only fields.
4. Preserve raw JSON. Compare only compatible modes, workloads and precision.
   Render traffic by operation separately from OS RAM and coverage limits. Show
   unreported rows or unknown coverage; do not calculate a retained-heap residual
   by subtraction or sum independent process/thread maxima.

Checks: actual upstream JSON fixture import, legacy provenance, rounded strings,
missing sections, configured truncation, environment overrides, malformed output,
child failures and incompatible comparisons. Existing `perf_memory`,
`perf_record`, `perf_compare`, lifecycle and partial-workflow suites should be
adapted to these contracts. Gate and commit.

### 1D. Infrastructure acceptance before another target run

Demonstrate an end-to-end controlled fixture exercising semantic-like work,
copying, persistence, workers and a child process through the existing store.
Review the coverage inventory, including all remaining unsupported questions.
Test disabled/timing/alloc feature combinations and run the common gates.

Exit: upstream resolution, operation reports, lifecycle, import, comparisons and
limitations are reviewable and committed. Then discuss readiness for a new cold
`elicit_doc` capture with both memory controls zero. Do not begin that run merely
because unit tests passed. Keep the target revision and capture configuration
fixed; preserve the old capture without pretending its fork-only peak/origin
metrics are comparable.

## Phase 2 — Establish a usable upstream measurement baseline

After the infrastructure review, measure `elicit_doc` through the existing perf
workflow. Produce an operation traffic ranking, OS RAM envelope, coverage table
and evidence-backed optimization order. Separate allocation churn from retained
storage hypotheses; traffic reductions alone do not prove a lower RAM peak.

Use controlled fixtures to assess instrumentation/report cost as operation row
counts grow. Use only documented upstream configuration. Report limits and
empty percentile displays are not evidence that recorder storage was reduced.
Remove the former local summary-mode, histogram and recorder-rewrite proposals.
If upstream overhead prevents useful measurement, stop at that concrete issue
and discuss an upstream request or supported configuration; do not fork again.
Gate and commit any consumer configuration changes independently of application
optimizations, and use a matching baseline after configuration changes.

## Phase 3 — Remove the graph-sized IR persistence copy

Introduce a borrowed serializable view of `CrateIr` in `src/ir/graph.rs`, retaining
the node/edge/index wire shape. Route `src/run_cache/store.rs` through that view
instead of deep-cloning with `snapshot()`. Initially accept synchronous writing
if borrowing requires it; measure the throughput tradeoff. Keep owned snapshots
where ownership is required, using a common wire implementation.

Checks: decoded old/new wire equality, graph identity and cache round trips;
failed/interrupted writes, partial-file cleanup and atomic publication. A large
attribute fixture should remove the extra payload-sized allocation traffic.
Inspect lifetimes to establish the removed overlapping representation; validate
whole-process RAM separately. Gate and commit.

## Phase 4 — Share immutable facts and eliminate conversion copies

1. Share immutable packed attribute bytes in `src/ir/node.rs` across memoized
   items and graph nodes. Clones start with empty decoded caches. Preserve
   mutation isolation, repeated-key/latest lookup and wire data.
2. In `src/loader/database/items.rs`, borrow JSON values and target syntax, then
   carry typed facts through flow resolution and encode once at the IR boundary.
   Reuse facts during projection population. All consumers use the shared loader
   and `IrView`; no side scan or parallel fact cache.
3. Reuse borrowed/shared evidence across consumers and remove unnecessary
   policy/context copies. Preserve distinct context-dependent results.

Checks: `ir_attribute_packing`, semantic recording/callees/backend, flow/context
and apply fixtures. Test clone/mutate/drop semantics and increasing consumer
counts; Hotpath should show reduced payload copying/conversion traffic.
Gate and commit each of the three steps separately.

## Phase 5 — Reduce persistence temporary representations

First measure Cordial's Salsa save boundary and inspect overlapping flattened,
pending and serialized dependency representations. Prefer borrowing/consuming
edges and serialization adapters over a second full representation. Preserve
shared-set deduplication, streaming, wire compatibility or explicit invalidation.
Salsa dependency changes require a separately reviewed scope; permission to use
upstream Hotpath grants no permission to extend another dependency for profiling.

Checks: flatten/save/reload, high-fanout and diamond dependencies, shared sets,
tracked identity, lazy/spill behavior, cfg contexts and direct/transitive edits.
Results and invalidation must match fresh execution. Increasing edge counts
should demonstrate less copying via upstream allocation metrics. Gate and
commit independently justified changes.

## Phase 6 — Release semantic working sets at proven last use

Use operation profiles, query counters and code lifetime analysis to identify
disposable parse, expansion and inference scaffolding. Record producers, last
consumers, persisted representations and recomputation costs; do not invent a
per-cache retained-byte table from traffic totals. Preserve compact semantic
results, necessary definition maps, dependency order and needed-crate closure.

Release intermediates after their last consumer even with unlimited budgets.
Share only context-invariant inputs after checking source/cfg/edition/macro and
dependency identity. Reuse existing memoization rather than adding another
cache. Threshold tuning or omitted semantic work is not an algorithmic win.

Checks: semantic workspace/backend/demand, cfg/context reuse/parsing/crates,
saved contexts, macro changes and transitive invalidation. Compare cold/warm
results and recomputation; exercise zero and bounded RSS budgets under the
explicitly accepted pressure contract. Gate and commit each cache/lifetime win.

## Phase 7 — Verify combined RAM reduction

Run the combined focused checks and common gates. With measurement readiness
accepted, repeat the matched cold unlimited-budget `elicit_doc` workflow in the
existing store. Compare supported allocation metrics, OS RSS high-water,
elapsed time, query counts, persisted size and output equivalence. Keep profiler
configuration fixed and distinguish application savings from profiler changes.

Publish the operation-by-operation findings, shipped/disproved design wins and
remaining coverage gaps in the existing perf record. No sum of local peaks,
exact ownership claim, promised 1.5 GB target, or Cordial self-run. Discuss any
return to self-analysis separately after the results justify it.

## Historical evidence

The first cold capture used Cordial `65cbde11` against `elicit_doc` `8d2968a7`,
with both controls zero: 168.49 seconds and pipeline OS high-water
3,366,715,392 bytes. Its [analysis and raw links](/home/erik/.cordial/elicit_doc/perf/1795226257-8d2968a7-memory-analysis.md)
remain immutable in the existing store; no baseline was promoted.

Its allocation-origin and requested-heap numbers came from the rejected fork.
They identify hypotheses behind the candidate optimizations, but do not define
upstream Hotpath capabilities or a compatible upstream baseline. The committed
phase-1A extension (`ea7cc38e`) and uncommitted phase-1B additions are work to
remove in phase 1A above, not accepted completed checkpoints. The earlier
[measurement plan](memory-measurement.md) is historical wherever it requires
those extensions.
