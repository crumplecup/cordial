# Memory measurement and reduction using upstream Hotpath

Status: **Upstream restoration, phases 3–4, narrow phase 5–6 reductions, phase 7 cold measurement review, getter scope reduction, query-owned RA definition maps and crate/tracked-output/module-declaration/item-tree retirement complete; other location/composite keys, bounded input/identity storage and stable IR identity remain, 2026-11-21.**
The immediate checkpoints restored published Hotpath with focused consumer
tests and removed the graph-sized serialization copy. Existing measurement
gaps are accepted and documented; they do not block application improvements.
Later instrumentation items remain optional follow-ups where they answer a
specific optimization question. The authorized unlimited cold `elicit_doc` run
has completed; continue with focused fixtures before discussing another run.

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
- Each implementation checkpoint requires focused tests under `tests/` run
  through Nextest for process isolation and parallel scheduling,
  `cargo fmt --check`,
  `cargo clippy --all-features --all-targets -- -D warnings`, and
  `git diff --check`, followed by a commit. A failed gate blocks the commit.
  Run Cargo invocations sequentially; use Nextest to schedule parallel tests
  rather than stacking commands that contend for Cargo's locks.
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
below. The later phase-7 checkpoint records the authorized full-program run.

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

## Checkpoint evidence — move context fallback facts

Phase 4 step 3 removes the selected context function's deep clone from source
call resolution and tracing projection. Both consumers use one `IrView` reader
that moves the selected function out of the decoded context vector. Ordinary
facts retain precedence, recorded context order stays unchanged, and the full
vector is still validated, including entries after the selected context.

The same two-context fixture, with a 256 KiB selected function, reported
**513.1 KB before versus 256.9 KB after** through published Hotpath. These are
rounded cumulative allocation-traffic values, not retained heap or RSS. Other
context facts are still decoded when required for validation; contexts and
their rust-analyzer databases are not merged or borrowed across boundaries.

Validation covers cold/warm reads, wire round trips, ordinary precedence,
malformed later entries, context isolation, call resolution, tracing and apply
snapshot consumption. Saved-context cases are checked with Nextest so each
test reads only its own process-global notes.
All-features/all-targets Clippy with warnings denied, formatting and diff checks
passed. No Cordial CLI or full-program workload ran.

## Checkpoint evidence — share policy reads and release decoded inventories

Phase 4 step 3 is complete. All ten policy enrichers read the same validated
`Arc<SourcePolicyInventory>` through `IrView`. The cache lives with the existing
packed attribute; graph clones still share only immutable bytes and start with
cold, independent decoded caches. Original wire data, unknown fields, latest-key
lookup and full typed validation are preserved. Concurrent cold readers may
temporarily decode more than once, but receive the same canonical handle.

Normal and on-demand loading release decoded root attributes after enrichment.
The release scans only the root, leaves encoded evidence available to later
readers, and preserves handles a consumer has explicitly retained. A fixture
observing only a weak handle confirms that normal loading releases the inventory
before probes. The on-demand release is covered by code inspection rather than
a separate runtime fixture.

The same 256 KiB policy fixture ran through unchanged upstream Hotpath:

| Operation | Before | After |
| --- | ---: | ---: |
| Ten sequential policy consumers | 2.5 MB | 260.2 KB |
| Construction and ten consumers | 3.3 MB | 1.0 MB |
| Four graph clones | 2.8 KB | 2.9 KB |

These rounded cumulative allocation-traffic values overlap and must not be
added. They demonstrate avoided repeated decoding, with a small cache-container
cost per graph clone; retained heap, process RSS and whole-program throughput
remain unmeasured.

Validation includes shared-handle identity, concurrent reads, cache/mutation
isolation, unknown wire fields, malformed inventory fields, round trips and
release before probes. Allows-only fixtures and the no-default library check
passed. All-features Nextest, Clippy with warnings denied, formatting and diff
checks passed. The saved-context query test permits only the one source-root
membership query needed by copy selection; it still rejects other analysis in
that step. No Cordial CLI or full-program workload ran.

## Checkpoint evidence — measure database persistence before changing it

`saved_state_allocation` measures the real save/restore boundary on the existing
three-file identity fixture with two macro-generated functions. Cold workspace
loading happens before capture; capture spans cold projection/save, database
teardown and restored loading/projection. The fixture checks callee and result
type identities after restore. It uses upstream Hotpath and the existing capture
pipeline in a disposable test store, with no full-program workload.

| Fixture boundary | Cumulative allocation traffic |
| --- | ---: |
| `SavedState::save` | 108.5 MB |
| `SavedState::restore` | 340.8 MB |
| `SavedState::read_part` (five parts) | 97.2 MB |
| `SavedState::decode_salsa` | 242.1 MB |

Units and rounding are preserved from Hotpath. Rows overlap and must not be
summed; these are allocation traffic, not retained heap or RSS. The encoded
Salsa file is 101,391,215 bytes; the other four parts total 488,343 bytes. Restore
holds that entire encoding while rebuilding the database. Save already streams
its encoding to a file; metadata buffers are much smaller in this fixture.

A buffered-reader restore experiment compiled, but the warm-load assertion
failed: `invalid type: string "universe", expected a borrowed string`.
`vendor/salsa/src/function.rs` requests map keys with `next_key::<&str>()`.
The unsuccessful Cordial change was removed. There is no claimed RAM saving
from this checkpoint, and no dependency code was changed.

The next reviewable option is to allow owned or borrowed map keys in Salsa's
ingredient decoder, then retry buffered restore and compare this same fixture.
That dependency change requires separate scope review under phase 5. Do not
work around it with a second decoder, unsafe buffer lifetimes, or a new fork.
Truncated and missing Salsa-file fixtures also check cold fallback after a
partially restored database is discarded. All nine tests in `saved_state` and
`saved_state_allocation` passed through Nextest. Formatting, all-feature,
all-target Clippy with warnings denied, and diff checks passed. Each Cargo
command ran only after the previous one exited. No Cordial CLI or full-program
workload ran.

## Checkpoint — Let Salsa read keys from slices or readers

The separately reviewed dependency scope is limited to accepting borrowed or
owned ingredient-map keys. A private transparent key type uses `Cow<str>` with
Serde's explicit borrowing annotation. Its generated visitor keeps slice-backed
keys borrowed, accepts temporary strings from reader buffers, and moves owned
strings when the deserializer supplies them. Each key is dropped after its map
entry; it is not retained in the database. No encoder or wire format changes.

The shared-set dependency fixture now restores JSON and MessagePack from both
borrowed input and seven-byte buffered readers. The reader case first reproduced
the borrowed-string failure without the decoder change. Existing checks cover
repeated restores, selective invalidation after direct and shared-input edits,
and tracked-field cutoff behavior. A reader-specific lazy-value fixture confirms
that restore decodes no memo payloads, reads reuse saved queries, resaving keeps
the exact MessagePack bytes without encoding values again, and a later edit
still invalidates what it reaches. Thirty-one Salsa persistence tests and nine
Cordial saved-state tests passed through Nextest across sequential Cargo runs.
Formatting, all-feature/all-target Clippy with warnings denied, and diff checks
passed. Cargo jobs ran sequentially; no Cordial CLI or full-program workload ran.

Cordial still reads the complete Salsa file before decoding. This adjustment
only enables reader compatibility; it does not itself remove that buffer or
establish a RAM reduction. Buffered Cordial restore and a matched allocation
comparison remain a subsequent checkpoint.

## Checkpoint — Restore Salsa through bounded input

Cordial now passes the Salsa file through a 64 KiB buffered reader rather than
retaining its entire encoding alongside the restored database. The four small
metadata files keep their existing slice decoders. Build fingerprint checks,
the saved wire format and cold fallback remain unchanged. The enabled reader
path uses the separately reviewed Salsa key adjustment above.

The same three-file, macro-bearing allocation fixture reports the following
upstream cumulative allocation traffic. Values retain Hotpath's displayed
precision and units; nested rows overlap and must not be summed.

| Operation | Whole-file input | Buffered input |
| --- | --- | --- |
| `SavedState::save` | 108.5 MB | 108.5 MB |
| `SavedState::restore` | 340.8 MB | 252.5 MB |
| `SavedState::read_part` | 97.2 MB, five calls | 477.2 KB, four calls |
| `SavedState::decode_salsa` | 242.1 MB | 250.5 MB |

The Salsa encoding is about 101.4 million bytes in both runs. Removing its
input copy saves about 88.3 MB of total restore allocation traffic, approximately
26%, after accounting for the reader decoder's additional allocations. The
reader uses a bounded input buffer, but this does not bound individual decoded
values or the database. The fixture confirms warm restoration and equivalent
projected identities; these numbers do not establish retained-heap or RSS
savings for a full program.

All 38 tests in `saved_state_allocation`, `saved_state`, `saved_contexts` and
`context_parses` passed through Nextest. They cover corrupt/truncated/missing
state fallback, stable identities, edits and invalidation, unchanged-query
reuse, context isolation, inactive files and restoration scaling. Formatting,
all-feature/all-target Clippy with warnings denied, and diff checks passed.
Cargo jobs ran sequentially. No Cordial CLI or full-program workload ran.

## Checkpoint — Release loader bundles after enrichment

The last-use audit found a lifetime boundary in the normal pipeline that does
not require evicting reusable queries or reordering hooks. Workspace source
facts and the call graph are collected before the enrichment/probe loop. Each
crate's enrichers are the remaining consumers of that crate's loader outputs;
snapshots and probes consume IR. Remove those outputs immediately after the
crate's final enricher, while keeping later crates' views available.

For source loading this releases the `SourceFile` strings read by
`SourceLoadView::for_target`; their facts are already projected into the shared
IR. For rustdoc it releases the owned inventory after graph population and
structure enrichment. Custom loaders follow the same borrowed-view contract.
The on-demand crate loader already scopes its local views to enrichment and
needs no change. There is no new cache or source scan.

The shared source database is still held by `ParsedSourceSession` until the
normal load-and-probe scope ends. Later targets and enrichers can still request
native syntax and semantic queries, so this checkpoint does not drop that
database or its parse/expansion/inference memos early. Pressure eviction,
saved-state semantics and hook ordering remain unchanged. Further semantic
scaffolding release needs evidence of its last consumers and recomputation
cost; allocation traffic alone cannot establish either.

A two-crate dependency fixture with two payload-bearing loaders first failed
at the first probe because its completed crate's payload remained alive. It
checks that all inputs survive every enricher, completed crates' payloads are
gone before probing, later crates' payloads remain available, and derived IR
facts and hook order hold on cold and cached runs. Weak references observe
actual object lifetime; no RSS reduction or allocation-traffic saving is claimed
from this fixture. All 12 focused Nextest cases passed across loader lifetime,
decoded-policy lifetime, gated semantic demand, crate attributes and apply
snapshot consumption. The Cordial self-analysis test was excluded. Formatting,
all-feature/all-target Clippy with warnings denied, and diff checks passed.
Cargo jobs ran sequentially. No Cordial CLI or full-program workload ran.

## Checkpoint — Semantic lifetime audit and combined verification

The remaining phase-6 candidates have different last consumers from the loader
bundles. The audit follows the existing owners and query paths:

| Representation | Producer and consumers | Release decision |
| --- | --- | --- |
| Per-crate loader bundles | `load_and_probe` loads them; workspace facts, call-graph setup and that crate's enrichers consume them. | Released after that crate's enrichers, as verified above. |
| Decoded root policy inventories | IR attribute decoding supplies shared inventories to enrichers; packed bytes remain the canonical representation. | Released before snapshots and probes by the earlier policy-lifetime checkpoint. |
| Parsed files and syntax maps | `SourceDatabase::collect_source` / `captured_source` obtain edition-aware parse and line-index queries; `SourceLoadView::with_syntax` also exposes this path to plugins. | A later crate or enricher may still request the same inputs. No new unconditional eviction is justified by this audit. |
| Macro expansions, arguments and span maps | Rust-analyzer queries build them during definition-map construction and function evidence; subsequent cfg passes and target projections can request them again. | Existing pressure-driven scaffolding release remains. Earlier release needs a fixture demonstrating both last use and recomputation cost. |
| Inference and definition maps | Priming, semantic evidence, cfg contexts and later dependent targets query the same database. Selected semantic results are projected into IR and saved query state. | Keep the shared database through its current session scope. Context-specific identities must remain distinct. |
| Run database and captured source | `CacheToken` owns the database; loader views share its token and `ParsedSourceSession` retains it until the run scope ends. | Releasing a view does not release the database. Resetting the slot early could cause later syntax reads or projections to recreate state; there is no proven benefit yet. |

This audit establishes no further last-use boundary for individual semantic
memos. It does not establish that those memos are inexpensive or small. Avoid
turning the accepted measurement gaps into another prerequisite for program
improvements: the shipped copy/lifetime changes are ready for combined
verification and the agreed measurement review. Further Salsa dependency-edge
flattening work remains a separate scope decision; the borrowed/owned key change
did not authorize redesigning its serializer.

All 26 combined Nextest cases passed across semantic backend/workspace,
context reuse/crates, cfg contexts, macro expansion, result-cache behavior and
memory pressure (133.745 seconds, four test processes). Checks include zero
macro re-expansion after restoration, cross-file and context-edit invalidation,
result-cache invalidation, equal graphs after spill/reload and under simulated
pressure, and the existing unlimited-budget behavior. Together with the 38
buffered-restore checks and 12 loader-release checks, this continuation passed
76 focused cases. Formatting, all-feature/all-target Clippy with warnings
denied, and diff checks passed. Cargo jobs ran sequentially. No Cordial CLI or
full-program workload ran. The next boundary is the agreed matched `elicit_doc`
measurement review; these fixtures establish behavior and local allocation
traffic changes, not a new whole-program RSS result.

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

## Checkpoint evidence — combined cold measurement review

The authorized single cold unlimited-budget `elicit_doc` workflow completed at
producer `eb2e9a6f`, using unchanged published Hotpath 0.31.2. Target `8d2968a7`
was clean and unchanged. Both memory controls were zero; the workflow reused
the existing PerfStore and `perf/work`, preserving historical captures and
baseline selection. No Cordial self-analysis ran.

The [analysis, raw capture and record links](/home/erik/.cordial/elicit_doc/perf/1795286265-8d2968a7-memory-analysis.md)
record **91.397 seconds** and a pipeline OS RSS high-water of
**2,815,610,880 bytes (2.622 GiB)**. All 1,088 upstream function rows were
imported without issues. The largest exclusive allocation-traffic rows were
definition-map construction (5.2 GB), semantic body walking (4.6 GB), Salsa save
(1.5 GB) and config-copy function discovery (803.8 MB), preserving upstream's
formatted units. They identify allocation-heavy work, not retained RAM owners.

RSS grew about 782 MiB between workspace loading and projection entry, then
another 1,269 MiB during ordinary projection. Config-context analysis ended
about 25 MiB above ordinary projection's endpoint; later work reached another
320 MiB above that context checkpoint. Phase samples cannot attribute that
later peak to a particular save, cache or diagnostic call. Storage diagnostics
themselves allocated 186.1 MB of traffic; that is not a retained-byte estimate.

Compared with the historical fork capture, observed high-water was 525.57 MiB
lower (16.37%) and elapsed time about 77.10 seconds lower. Profiler and
application changes occurred together, and OS cache state was uncontrolled;
these observations do not isolate application savings. Fork retained-heap
metrics are incompatible with upstream. This capture supplies an upstream
reference without automatically promoting the baseline.

All 2,528 findings' scoring keys and per-rule counts match. Quality and summary
reports are byte-identical, with hashes in the analysis. Hook counters match.
Executed queries increased by 545 (0.092%): workspace -7, config context +552;
context query kinds increased from 86 to 89. Encoded Salsa size increased from
303,524,197 to 303,642,268 bytes. These small differences remain explicit;
output equivalence does not prove identical query behavior. The cold run does
not measure buffered restore's benefit. Earlier checkpoint fixtures remain
the evidence for graph wire format, apply and invalidation behavior.

### Next focused implementation order

1. **Remove unnecessary collector scope copies.** Getter collection clones
   binding/function maps at every syntax node, before establishing whether a
   scope boundary needs restoration. Save only at actual boundaries first;
   review conversion collector context copying separately. Exercise shadowing,
   let initializer order, closures, nested items, local types, macros, aliases,
   cfg behavior and apply round trips. Use a controlled many-bindings and
   many-expressions allocation fixture to verify the scaling improvement.
2. **Bound cfg-context lifetime.** This is the primary architectural target:
   Cordial's individually gated etiquettes need many contexts, so the small
   context increment in `elicit_doc` does not represent Cordial's workload.
   Follow the ordered context work below before unrelated collector work.
3. **Separate stable IR identity from context-local RA identity, then share
   invariant facts.** Follow the identity and reuse checks below. Equal source
   text or equal declaration identity alone does not imply equal semantic facts.
4. **Evaluate remaining persistence metadata copies.** Retain borrowed IR
   encoding and buffered Salsa output. Use high-fanout, diamond, shared-set,
   lazy/spill and invalidation fixtures. Broader Salsa dependency representation
   changes require a separate design review; no such redesign follows from
   this measurement alone.

Keep diagnostic overhead visible through existing APIs. The measurement is
sufficient to pursue these application wins; no additional profiling system
or instrumentation completeness effort is a prerequisite. Gate and commit
each implementation checkpoint under the existing contract. Discuss a later
full-program run separately.

### Getter scope checkpoint

Getter collection now saves its binding/function maps only at scope boundaries,
instead of cloning them for every syntax node. An unchanged-upstream Hotpath
fixture varies visible bindings and expression count independently and checks
the resulting receiver facts. Before the change, increasing expressions from
16 to 256 added about sixteen times as much allocation traffic with 128
bindings as with eight. After the change, that growth is approximately equal.
The 128-binding, 256-expression case fell from upstream's **49.4 MB** to
**632.9 KB** of exclusive collector allocation traffic. These are formatted
allocation totals, not retained-memory measurements.

The lexical-scope fixture checks nested blocks, shadowing, a let initializer,
closures, nested functions, impl receivers and local types. Existing getter
and apply round-trip fixtures remain the broader semantic regression checks.
All 22 focused Nextest tests passed across the two new targets and
`internal_error_chain_apply_getters_{consumers,conditional,aliases,tuples}` and
`internal_error_chain_apply_cross_file_round_trip`, with `hotpath-alloc`
enabled. Formatting, `cargo clippy --offline --all-features --all-targets --
-D warnings` and `git diff --check` passed. No Cordial self-execution ran.

### Ordered cfg-context work

**Current implementation, verified 2026-11-21.** `add_context_crates` appends
copied crate/file registrations and keeps every context in `context_crates`.
Unchanged files may alias ordinary files; copied files share their original's
text allocation. Each copied crate still has separate definition maps and
context-dependent query results. The last-consumer boundary now follows
observability, planning and optional persistence fingerprint preparation.
Retirement now runs there for copied crate keys, module-scope declaration keys,
locally defined macro keys recorded in the definition map, members and variants
recorded in already-current trait/impl/enum query results, body-local declarations
in current block maps reached through producer records, and tracked identities
created by queries on those keys, transitively. Local declarative macro
definitions also select their explicit expander keys. File and block item trees
select explicit location-and-crate keys from module origins and local macro
calls, including cached associated-item calls. Selected
non-persisted values are dropped, and persisted values spill when a run cache
is available, even without memory pressure. Other composite arguments are not yet
selected through their owners. Selection reads the current map and skips absent,
stale or provisional member/variant/block results; removed declarations from
earlier revisions and block maps missing producer records are not enumerated.
The existing broader eviction hook remains conditional and does not select
individual contexts.
Removing a `ContextCrates` handle alone would not free
Salsa's memo values, dependency records or interned identities.

The target working set is shared workspace/dependency state plus **one active
context**, with completed facts retained in the IR or existing persistence
store. Context summaries and distinct output facts may legitimately grow with
coverage; completed RA semantic working sets must not accumulate with them.

1. **Establish the last-consumer boundary.** Finish harvesting, observability
   merging and follow-up gate discovery before eviction. Include library and
   leaf passes, rejected merged contexts, partial failures and resumable stage
   boundaries. Correct the stale module description that already claims only
   one variant is resident. Check `cfg_contexts`, `context_crates`,
   `context_reuse`, `context_parses`, `saved_contexts` and tracing context
   fixtures before committing the lifecycle change.
2. **Prove selective retirement on a controlled context family.** Use 1, 4 and
   16 mutually exclusive cfg variants with shared dependency work. Inspect the
   existing Salsa memo counters to distinguish context-only results from
   shared results. Check that the previous context's regenerable values are
   released after its final consumer, shared results remain reusable, rejected
   contexts are released, and re-entry computes the same answers. Existing
   Salsa eviction selects query kinds, not context keys; establish how to
   select the context's memos before changing storage. Do not describe a global
   cache flush, removing a handle, or unregistering a root as selective
   retirement. Any required Salsa API/dependency representation change gets a
   concrete design review before implementation.
3. **Make identity independent of transient RA handles.** Inventory every
   semantic identity crossing into the IR, external targets, leaf facts and
   saved progress. Extend the existing source-description identity path rather
   than adding a second resolver. Today `ItemKey` interns descriptions in
   Salsa; canonicalization joins a copy to its ordinary declaration only when
   that declaration resolves there. Cfg-only declarations retain copy-specific
   descriptions. Separate a declaration's stable IR identity from the active
   context's RA lookup identity, including cfg-only declarations. Never use a
   dead context's `Function`, `ModuleDef`, `FileId` or macro-expansion handle to
   answer a later context. Verify context-order independence, discard/re-entry,
   save/restore, edits, same-named mutually exclusive declarations, crate and
   edition distinctions, and generated items with different macro provenance.
4. **Complete bounded context storage and share proven invariant facts.**
   Retire or reuse copied inputs and query storage without invalidating
   unrelated ordinary/dependency state. Handle public owned `ContextCrates`
   values explicitly so stale handles cannot silently refer to a reused slot.
   Persist compact facts with stable identity before releasing their RA
   context. Reuse a semantic result only with its cfg, dependency and macro
   validity evidence; share declaration identity independently of body facts.
   The many-context fixture must show bounded completed-context semantic memo
   residency, correct distinct call edges/types and unchanged ordinary facts.
   Check cold, reused, restored, edited and resumed paths, including leaves.
   Use Hotpath for traffic and existing OS readings for the process envelope;
   neither query counts nor traffic are retained-byte ownership estimates.

Each step has its own focused Nextest, formatting, all-feature/all-target Clippy
and diff gate, followed by a commit. No Cordial self-run or further full-program
measurement is part of these steps. Remaining conversion-collector and generic
semantic/persistence optimizations stay behind this context work.

**Step 1 checkpoint.** Library and leaf passes now finish observability and
follow-up discovery before asking for eviction, including rejected merged
contexts and harvest failures. Registration failures also reach the existing
eviction hook. When cross-edit persistence is enabled, each context prepares
its persisted fingerprints before eviction; otherwise the final save could
rebuild its semantic indexes. The final save reuses these fingerprints.
The lifecycle descriptions now distinguish shared source text from separate
syntax and semantic values. This checkpoint does not yet select context-only
memos or make retained RAM bounded.

All 51 focused Nextest tests passed with `hotpath-alloc`: `cfg_contexts`,
`context_crates`, `context_reuse`, `context_parses`, `saved_contexts`,
`tracing_print_contexts` and `tracing_apply_proof_context`. The new two-context
fixture checks fingerprint execution at the context boundary, reuse at the
final save and reuse after restoration. Cargo metadata subprocesses require
registry access even when the outer Cargo invocation is offline; the complete
successful run allowed that access. Formatting, all-feature/all-target Clippy
with warnings denied and diff checks passed. No Cordial self-execution or
full-program measurement ran.

**Selective-eviction design to review in step 2.** Prefer extending the existing
Salsa eviction operation with a dependency-based selection: seeds are the
completed context's copied crate inputs, source-root inputs and copied file
inputs; select regenerable memos whose recorded input dependencies reach those
seeds. Include transitive reads and cycles, without executing queries to
classify them. Determine the selection before dropping values. Leave inputs,
interned identities, dependency records and persisted evidence intact in this
first change, so existing handles remain valid and a dropped value can be
recomputed. Exercise shared work first computed inside a context as well as
shared work warmed beforehand; creation time is not ownership. A persisted
value must remain available or use the existing spill path, never disappear
from the next saved state. Compare classification cost as context count grows;
do not introduce a second dependency graph or per-context copy of shared sets.

This would release the large semantic values without yet bounding input,
interning or memo-header storage. Report that remaining growth separately.
If recorded dependencies cannot distinguish context-only work from shared work
reliably and cheaply, review explicit context ownership or database replacement
with its recomputation cost before choosing an implementation. Do not silently
fall back to flushing every query or mutating ordinary cfg inputs.

**Step 2 storage gap, verified in source and a fixture, 2026-11-21.** Query memo selection
alone cannot release every large context payload. In
`vendor/ra_ap_hir_def/src/nameres.rs`, `crate_local_def_map` is a persisted
query returning a `DefMapPair` tracked-struct handle. The tracked struct owns
the `DefMap` and `LocalDefMap`; the function memo owns the handle. Salsa's
current `values_in_memory` and `spill_persisted` implementations visit function
memos, not tracked-struct fields. Spilling that query therefore spills its
handle and leaves the maps in the tracked-struct table. Block definition maps,
which are direct query values, have a different ownership path.

The `salsa_context_retention` fixture models these two paths using 1, 4 and 16
context inputs and weak lifetime markers. After global regenerable eviction
and persisted spilling, original direct-query payloads are released,
while tracked-struct payloads remain until the database is dropped. Encoded
data stays in the fixture's in-memory store; this checks original-object
ownership rather than total heap bytes or RSS. This fixture must become a
release/re-entry check when the storage gap is addressed.

The step-2 design review must explicitly cover this payload storage, before
implementing dependency selection:

| Candidate | Concrete change and checks | Tradeoff |
| --- | --- | --- |
| Move RA map payloads into query-owned values | Replace the payload-bearing tracked `DefMapPair` with query-owned data; retain module/block identities. Prove external-map versus local-map invalidation, module identity after recomputation, persistence and stable fingerprints. Then exercise release through the existing spill store. | Narrower storage change, but field-level backdating and the saved representation require care. Do not silently discard incremental reuse. |
| Spill tracked payload fields within Salsa | Keep tracked identity/revision records and add lazy field storage through the existing spill store. Prove field borrowing, update/invalidation, restore and failure behavior before connecting context selection. | Preserves RA's data model but broadens Salsa's storage and unsafe lifetime obligations. This is a larger change and needs explicit review. |
| Discard a separate context database | Materialize stable IR facts, then drop the entire database. Verify that no RA handle crosses the boundary. | Bounds that database's working set but may rebuild shared dependencies for every context; compare that cost before choosing it. |

Prefer investigating the narrow RA payload change first, with the current
tracked module/block identities retained. A concrete candidate is a persisted,
ordinary query result owning `Arc<DefMap>` and `Arc<LocalDefMap>`, with separate
tracked projection queries for external and local reads. The projections would
clone the `Arc`, not the maps, and independently backdate equal results. Their
non-persisted memo values must be evicted as well as spilling the owning result;
otherwise a projection can keep the maps alive. Keep module creation in the
same owner query so that changing payload storage does not deliberately change
its tracked identity path. This is a candidate to validate, not an assertion
that the existing invalidation and identity guarantees follow automatically.
The review checks must include a local-prelude-only edit that leaves an
external consumer's result reusable, a public-definition edit that invalidates
it, ownership release after all projection references are evicted, re-entry
with equal module identities, and cold/restored fingerprint equality. A plain
pair replacement without these independent projections could broaden
invalidation and is not the proposed implementation.

The context-selection API remains necessary for
other semantic memos and for retaining shared ordinary/dependency work. No
storage redesign or new Salsa selection API has been implemented at this
checkpoint. All 12 focused Nextest tests passed across `salsa_context_retention`,
`salsa_eviction`, `salsa_spill` and `salsa_tracked_identity`, with `hotpath-alloc`.
Formatting, all-feature/all-target Clippy with warnings denied and diff checks
passed. No Cordial self-execution or full-program measurement ran.
Stable IR identity remains the next ordered architectural step;
stable identity by itself does not release a tracked payload or prove that a
semantic fact is invariant across cfg contexts.

**Query-owned storage exploration, 2026-11-21.** The controlled
`salsa_context_retention` fixture now exercises an ordinary persisted owner of
two `Arc` payloads, separate non-persisted external/local projection queries,
and small persisted tracked module identities created by the owner. Projection
reads share the owner's allocations. The candidate uses the existing Salsa
APIs and spill store; it introduces no Salsa storage or unsafe lifetime change.

The checks distinguish three mechanisms:

- **Release and re-entry:** across 1, 4 and 16 inputs, evicting regenerable
  projections and spilling persisted owners releases every original payload
  marker. Re-entry decodes equal contents and module identities without
  executing the owners, and those decoded payloads can be released again.
- **All references matter:** spilling the owner alone leaves both original
  maps alive through their projection memos. Evicting those memos releases the
  remaining references. An implementation must include both ownership paths
  in retirement, regardless of which is visited first.
- **Independent invalidation:** local-only edits rerun the owner and local
  consumer, while the external consumer remains cached; external-only edits
  have the converse behavior. These checks cover warm, spilled-owner and
  saved/restored owners. Consumers and projections are warm during the edits:
  retiring a consumer's value would itself require recomputation. This does
  not establish persisted-consumer cutoff behavior after an edit.

The identity fixture also forces owner recomputation, rather than decoding,
in cold and restored databases, while a separate persisted query retains the
module identity. This is needed because successful decoding alone does not
exercise tracked module creation a second time. Cold eviction forces execution;
the restored case edits an input after eviction because snapshot bytes would
otherwise allow decoding. Module identities remain equal in both cases.

All 16 focused Nextest checks passed with `hotpath-alloc`, across
`salsa_context_retention`, `salsa_eviction`, `salsa_spill` and
`salsa_tracked_identity`.
Formatting, all-feature/all-target Clippy with warnings denied and diff checks
passed. No Cordial self-execution or full-program measurement ran.

The production transition remains narrow: replace `DefMapPair`'s payload-bearing
tracked struct with an ordinary query value owning the two maps through
`triomphe::Arc`; retain the `crate_local_def_map` owner query and module/block
creation paths. Route external-only access through an independently tracked
projection and local-only access through its counterpart. Combined readers
and the existing combined stable fingerprint can read the owner. Keep each
map encoded once in the persisted owner; non-persisted projections add Arc
references rather than a second persisted copy of a map. Validate actual RA
collection, public/local changes, module identity, fingerprints and context
save/restore before treating this fixture result as a production guarantee.
There is no production RA storage change or per-context selector at this
exploration checkpoint, and payload lifetime markers are not retained-byte or
RSS measurements.

**RA query-owned storage checkpoint, 2026-11-21.** The production owner query now
returns an ordinary `DefMapPair` owning `triomphe::Arc<DefMap>` and
`triomphe::Arc<LocalDefMap>`. Module/block creation stays in the same owner
query. External-only and local-only readers use separate non-persisted tracked
projections; combined readers and the stable fingerprint read the owner. The
maps themselves are not cloned or separately persisted by these projections.
This uses the existing Salsa APIs and spill store, with no Salsa storage or
unsafe lifetime changes.

The new `ra_def_map_storage` tests exercise actual RA collection on two tiny
in-memory crates. Across cold/restored and resident/spilled owners, adding an
unused dependency changes the local prelude while a warm external consumer
stays cached. Adding a public function reaches that consumer. Both changes
preserve existing module identities. A separate saved-consumer fixture uses
the loader's cutoff and view-initialization protocol: body edits preserve the
fingerprint and reuse the saved consumer, while public edits change its value.
Cold/restored fingerprints and module identities agree. The native spill check
first populates both projections, finds an encoded map owner and no separately
encoded projection, observes no resident owner/projection memo values after
retirement, then reads equal
contents, identities and fingerprints without running collection again. It can
retire those decoded values a second time. These are memo/identity checks;
original-allocation release is established by the controlled ownership fixture,
not by a native retained-byte or RSS measurement.

Warm backdating and saved cutoffs have different boundaries. Saved consumers
still use the existing combined external/local fingerprint after dependency
flattening. Local-only changes can therefore invalidate saved external readers
conservatively even though warm external readers remain cached. This change
does not introduce separate external/local saved cutoffs. The persisted owner
representation changes; existing saved-state build fingerprints reject caches
from a different executable before decoding.

The full 70-test Nextest run passed 69 checks; its saved-query accounting check
reported only the new non-persisted external projection under map priming. That
check now admits precisely that projection with the same count as the unchanged
cold run, while continuing to reject actual map collection or other analysis
there. All 9 affected checks then passed, covering that assertion, the three
native RA checks and the five controlled ownership checks. The complete suite
covers cfg collection, context crates/reuse/parsing, saved contexts, tracing
prints/apply, Salsa ownership, eviction, spill and tracked identity, with
`hotpath-alloc`. Formatting, all-feature/all-target Clippy with warnings denied
and diff checks passed.

This storage checkpoint enabled map payload retirement without yet selecting
one completed context or bounding input, identity and memo-header growth.
The direct-key checkpoint below adds that first selection. No Cordial
self-execution or full-program measurement was part of this checkpoint.

**Direct-key retirement design, 2026-11-21.** Source review found a limit in
the dependency-only proposal: immutable tracked identity fields add no input
edge. A query keyed by a context's module can therefore hold a separate large
value without reaching any of that context's input seeds. RA's `ModuleIdLt`
uses these identity fields. The controlled ownership fixture checks this shape
with 1, 4 and 16 contexts and an ordinary module, including save/restore. This
does not establish how many production RA values have that shape.

The first selective retirement should release the known map payloads without
waiting for a complete ownership classifier:

- Extend the existing Salsa value inspection/drop and persisted spill walks
  with an optional set of exact input keys. Keep the existing unfiltered APIs
  and the same memo lifetime/exclusive-access rules. An empty selection selects
  nothing; it must not mean a global flush.
- At the last-consumer boundary, select the completed context's copied crate
  inputs. The map owner and both projections use these keys directly. Drop
  selected non-persisted values and spill selected persisted values through the
  existing store. Do not delete inputs, identities, edges or saved evidence.
- Open the existing spill file at this boundary when a run cache exists, even
  without memory pressure. Without a store, preserve persisted owners rather
  than discarding them. Preserve ordinary/dependency map values, including
  shared work first computed while processing a context.
- Check original map payload release and shared residency for 1, 4 and 16
  contexts, empty selections, repeated retirement, re-entry and saved/restored
  state. Check native RA owner/projection residency and module identity too.
  Select input IDs directly within compatible key ingredients instead of
  scanning all registered key slots for each context. Memo residency counts
  are not a retained-byte measurement.

This is direct-key map retirement, not complete context retirement. Queries
keyed by files, modules, interned locations or argument tuples are not selected
merely because they read a copied crate. The next ownership design must account
for these keys as well as input dependencies, using existing producer/output
records where available. It must not infer ownership from creation time or
copy a dependency graph for each context. The ordered identity and bounded-input
work remains after this step.

**Direct-key map retirement checkpoint, 2026-11-21.** Implemented the design
above through the existing Salsa drop/spill walks and the existing run-cache
spill file. The last-consumer boundary selects only copied crate input IDs;
selection checks compatible key ingredients and does not enumerate every
accumulated key slot. No input, identity, dependency edge or memo header is
deleted. Without a store, persisted owners remain resident and saveable.

The controlled 1/4/16-context fixture releases every original context map
allocation while retaining ordinary and dependency map allocations, including
dependency work first used during a context. Empty selections do nothing.
Repeated retirement and cold/saved re-entry preserve module IDs and answers
without rerunning map owners. The native two-crate RA fixture confirms owner
and projection residency drops only for the selected crate, with shared maps
still resident and re-entry requiring no definition-map collection. Restored
`lang_items` decodes without warming its local projection; the native check
expects that precise residency difference.

The immutable-module-key fixture also confirms the dependency-only ownership
gap in cold and saved/restored state: separate module-keyed payloads can have
empty saved input dependencies. Step 2 remains partial until indirectly keyed
values have an ownership rule covering tracked identities and composite keys.
These tests establish original allocation lifetime and memo residency, not a
production retained-byte total or bounded whole-context storage.

Validation passed: all 74 tests across the 12 focused storage, cfg-context,
saved-state and tracing/apply binaries with `hotpath-alloc`; formatting;
all-feature/all-target Clippy with warnings denied; and diff checks. No Cordial
self-execution or full-program measurement was used.

**Tracked-output selection design, 2026-11-21.** Extend the exact-key selector
using the tracked identities already recorded in each memo's query revisions.
Starting with the copied crate IDs, inspect only the query ingredients attached
to each selected key's struct ingredient. Add the tracked structs those queries
created, and repeat for newly selected keys. Deduplicate keys in a temporary
worklist; do not copy dependency edges, execute queries, decode values, or scan
all allocated slots. Keep the existing direct-key drop/spill APIs unchanged.

Review of the storage path confirms that these producer lists survive value
dropping, spilling and saved-state serialization. They describe identities
created directly by the query, rather than identities merely read or returned
from another query. This distinction preserves shared dependency work first
warmed during a context. Follow only these producer records, not ordinary read
edges or specified-query outputs whose keys may belong to shared storage.
Collect the complete selection before dropping any values, with exclusive
database access and the existing memo inspection lifetime rules.

Check 1/4/16 contexts, nested tracked outputs, shared work created during a
context, empty/duplicate seeds, repeated retirement, original allocation
release, re-entry and cold/saved state. Also exercise actual RA module-keyed
values alongside crate maps. This extends retirement to direct tracked-output
keys; it does not select interned locations or argument tuples by their fields,
recover absent non-persisted producer memos after restore, or reclaim inputs,
identity fields, memo headers or replaced memos. Step 2 remains partial.

**Tracked-output retirement checkpoint, 2026-11-21.** Implemented the design
above with an exclusive-access key collector over existing memo producer lists.
It visits registered query ingredients only for selected struct keys, follows
their tracked outputs transitively and deduplicates the temporary worklist.
There is no new ownership graph, serialized field or slot scan. The existing
last-consumer retirement path collects these keys before its existing drop/spill
walks; without a store, persisted owners remain intact as before.

The 1/4/16-context fixture releases original persisted and regenerable payloads
keyed by modules and nested tracked outputs. Ordinary/dependency values remain
resident, including dependency work first created inside a context and a shared
identity returned by a context-keyed query. Empty/duplicate selections, repeated
retirement, saved re-entry without producer execution, key collection without
decoding values, and changed producer identities after edits all pass. The native RA
fixture selects real module identities produced by the definition-map owner,
retires their test payloads alongside maps, preserves dependency module/map
values and re-enters with the same identities without definition-map collection.

All 76 tests across the 12 storage, cfg-context, saved-state and tracing/apply
binaries passed with `hotpath-alloc`. Formatting, all-feature/all-target Clippy
with warnings denied, and diff checks passed. No Cordial self-execution or
full-program measurement was used. These checks establish original allocation
lifetime and memo residency, not a retained-byte total. Interned locations,
composite keys, absent non-persisted producer records after restore and bounded
input/identity/header storage remain outside this checkpoint; step 2 is partial.

**Declared-location selection design, 2026-11-21.** Use ownership facts already
stored in each completed crate's definition map to extend the seed keys. Module
scopes distinguish declarations, impls, unnamed constants, use/extern
declarations, built-in derive impls and local macro calls from imported names.
Expose an iterator over those existing keys, including the map's own modules;
do not walk visible namespace entries, inherited legacy macros or dependency
maps. Feed the seeds through the existing tracked-output collector and the
existing exact-key drop/spill path. This needs neither an intern-table scan nor
a second ownership index or macro dependency change.

Read the active crate map through its persisted owner before retirement,
without recreating the non-persisted external projection merely to enumerate
keys. This can decode a spilled map; it is an ordinary RA query read, not a
guarantee of allocation-free selection. A failed context with no map yet may
collect one. The iterator itself only reads stored facts and runs no queries.
Check native interned function keys and real body queries, dependency imports/re-exports,
local macro-generated declarations, repeated retirement, edits and saved
re-entry. Check scope keys across 1/4/16 cfg copies, keeping ordinary and shared
dependency payloads resident. Associated items, enum variants, non-exported
legacy macro definitions, body-local block maps and arbitrary composite keys
remain separate gaps; module-level declarations do not establish retirement
of every context value.

**Declared-location retirement checkpoint, 2026-11-21.** Context completion now
seeds retirement with the existing crate map's modules and recorded local
declaration/location keys, before following tracked outputs. This reaches
interned function keys, including functions generated by local macro calls,
and RA body-query values. Imported/re-exported dependency definitions are not
selected merely because they are visible in a completed context. Selection
reads the persisted map owner directly: a restored run need not execute the
non-persisted external projection solely to enumerate declaration keys.

The native RA fixture exercises 1, 4 and 16 cfg copies in cold and saved/restored
databases. Each copy has a selected function, a nested function and a
macro-generated function. It warms real body queries and persisted item
payloads. Ordinary work stays warm; dependency payloads are first warmed inside
a context. After each retirement, completed copies have no resident values for those two
query kinds and their original payload markers are released. Ordinary,
dependency and uncompleted-copy values remain resident. Repeated retirement
is safe; re-entry restores the same item identities and payloads without map
collection. An edit check selects replacement declarations from the current
map, excludes removed declarations and preserves the ordinary copy.

All 78 focused Nextest tests passed across the 12 context, persistence, Salsa,
map-storage and tracing binaries with `hotpath-alloc`. The saved-query
accounting assertion remains unchanged and passes; the final save still reuses
prepared fingerprints. After the test-loop Clippy correction, all six native RA
checks passed again. Formatting, all-features/all-targets Clippy with warnings
denied and diff checks passed. No Cordial self-execution or full-program
measurement ran. These checks establish value residency and original payload
release in fixtures, not retained-byte or whole-program RSS savings.

Step 2 remains partial. The current map does not enumerate removed declarations
from earlier revisions, associated items, enum variants, non-exported legacy
macro definitions or body-local block declarations. Arbitrary composite keys,
absent non-persisted producer records after restore and input/identity/header
storage also remain. The next member-key case needs care: RA's `TraitItems`
and `ImplItems` already hold associated item IDs, but their queries are not
persisted. Unconditionally reading them after restore could perform syntax,
attribute and macro analysis merely to select values for retirement. Cover
that path in a native fixture before extending selection; preserve the saved
reuse checks rather than broadly allowing new retirement-time analysis.

**Associated-item design review, 2026-11-21.** Keep RA's existing `TraitItems`
and `ImplItems` results as the only member lists. Make those query values
persistable with their existing diagnostics and macro-call records. At
retirement, enumerate only locally declared trait/impl owners from the current
persisted crate map. A narrow Salsa metadata selector will return owners whose
named query already holds a value (resident or encoded), is verified in the
current revision and is final. Reading these results can decode saved data but
must not collect members. Absent, stale or provisional results are skipped;
normal analysis can warm them before a later retirement. No intern-table scan,
ownership index, macro-library fork or erased output-value visitor is needed.

The selector uses exclusive database access and the same exact-key memo lookup
as tracked-output selection. It does not validate memos or follow dependencies.
Tests must cover cold and restored 1/4/16 families, actual method bodies, cfg
members, macro-generated members, ordinary/shared work, repeated retirement,
same-ID re-entry, edits and untouched owners. Existing saved-query accounting
assertions stay intact. This expands value retirement only; input/identity
storage and other ownership gaps remain.

**Associated-item retirement checkpoint, 2026-11-21.** Context completion now
adds the associated functions, constants, type aliases and macro-call IDs
recorded by existing, current trait/impl query results. Local scope ownership
excludes imported/re-exported dependency traits and types. The existing member
queries now persist their results and diagnostics. A metadata-only exact-key
selector checks memo presence, verification revision and finality before any
member-list read; saved lists can decode without syntax or macro collection.
No ownership index or macro dependency change was introduced.

The native RA fixture covers 1, 4 and 16 cfg copies in cold and restored
databases, including default and required trait methods, inherent and trait
impl methods, cfg-gated methods and macro-generated members. Real body queries
and persisted item payloads release their resident values for completed copies;
weak markers confirm release of their original payload objects. Ordinary,
dependency and uncompleted-copy values remain resident. Repeated retirement
and same-ID re-entry reuse member lists without collection. A separate fixture
checks untouched and stale results: selection does not collect or validate
them; after normal analysis, replacement members are selected instead of the
removed declarations. Salsa selector tests check exact keys, duplicates, empty
selections, dropped/stale/missing values and encoded results without decoding.

All 82 focused Nextest tests passed across 13 binaries with `hotpath-alloc`.
Saved-query accounting assertions remain unchanged and pass. Formatting,
all-features/all-targets Clippy with warnings denied and diff checks passed.
No Cordial self-execution or full-program measurement ran.

Step 2 remains partial. Member selection deliberately skips absent, stale and
provisional results; it cannot recover removed declarations from earlier
revisions. Enum variants, non-exported legacy macro definitions, body-local
declarations, composite arguments, missing producer records after restore and
input/identity/header storage remain separate gaps. These fixtures establish
memo residency and original-object release, not retained-byte or whole-program
RSS savings.

**Enum-variant selection design, 2026-11-21.** Extend the existing declaration
selection with locally declared enum owners, using the same current/final memo
selector as associated items. Persist the existing `EnumVariants` result and
inactive-variant diagnostics, then read only already-current results to select
their interned variant IDs. Do not reconstruct variants from syntax, follow
imported enum names or collect absent/stale lists at retirement. Cover unit,
tuple, record, cfg-gated and macro-generated variants in cold/restored 1/4/16
families. Check actual discriminant-body and field-owner reference counts,
shared work, repeated retirement, same-ID re-entry, inactive diagnostics and
edits before the next commit.

**Enum-variant retirement checkpoint, 2026-11-21.** Local enum owners now use
the same current/final exact-key selector as trait/impl owners. Their existing
variant lists and inactive-variant diagnostics persist, so selecting variant
IDs after restoration can decode the existing result without collecting it.
Imported/re-exported dependency enums do not become local owners.

The native fixture covers 1/4/16 cold/restored families with unit, tuple,
record, cfg-gated and macro-generated variants. It warms actual RA
discriminant-body and field queries, including their field projection, and
keeps one test reference to each original body and field object. After each
retirement those test references are the sole remaining owners of completed
objects: RA has released every reference to them. Ordinary, shared and
uncompleted-copy objects retain database owners. Repeated selection/retirement
and same-ID re-entry preserve variant lists and inactive diagnostics without
collection. Separate edit checks skip untouched/stale lists, then select the
current replacement variants once normal analysis has warmed them.

All 84 focused Nextest tests passed across 13 binaries with `hotpath-alloc`.
After adding the explicit zero-residency assertion for completed variant lists,
all 10 native RA checks passed again. Saved-query accounting assertions remain
unchanged and pass. Formatting, all-features/all-targets Clippy with warnings
denied and diff checks passed. No Cordial self-execution or full-program
measurement ran.

These tests establish query residency and RA reference release, not retained
bytes or process RSS; Triomphe has no weak pointers, and the explicit test
references themselves keep the original objects alive until dropped. Step 2
remains partial for absent/stale/provisional lists, removed declarations,
non-exported legacy macro definitions, body-local declarations, composite keys,
missing producer records after restore and input/identity/header storage.

**Local legacy macro selection design, 2026-11-21.** The persisted `DefMap`
already records local macro definitions in `macro_def_to_macro_id`. Its
collector inserts macro rules, macro 2.0 and procedural definitions there;
inheritance and imports update separate namespace tables. Seed retirement from
this existing ownership table, including private, shadowed and macro-generated
legacy definitions. Do not walk every module's copied legacy namespace or
introduce another ownership index. Test ordinary/shared/uncompleted ownership,
cold/restored 1/4/16 families, cfg filtering, repeated retirement, same-ID
re-entry and edits. This covers exact macro-ID query keys; composite macro
expander keys remain outside this checkpoint.

**Local legacy macro retirement checkpoint, 2026-11-21.** Retirement now
includes the map's existing local macro-definition keys, without following
inherited or imported macro names. The native fixture covers private, shadowed,
nested, exported, macro-generated and cfg-gated macro rules across cold/restored
1/4/16 context families. It also verifies that the dependency macro is actually
imported and that legacy namespaces contain inherited entries.

Actual RA macro-definition and attribute-query residency drops to zero for
completed contexts, as does the persisted test payload keyed by each macro ID.
Weak markers verify release of the original test allocations. Ordinary,
dependency and uncompleted-context values remain resident. Repeated retirement,
same-ID re-entry, persisted payload contents and edited replacement definitions
pass without extra crate-map collection. These checks establish exact-key
residency and test-allocation release; they do not measure expander allocation
release, retained bytes or RSS.

All 86 focused Nextest tests passed across 13 binaries with `hotpath-alloc`,
including the unchanged saved-query accounting checks. Formatting,
all-features/all-targets Clippy with warnings denied and diff checks passed.
No Cordial self-execution or full-program measurement ran. Step 2 remains partial
for body-local declarations, composite keys, absent/stale/provisional owner
lists, removed declarations, missing producer records after restore and
input/identity/header storage.

**Body-local declaration selection design, 2026-11-21.** Body lowering creates
tracked block identities, and the existing tracked-output selector reaches those
keys without reading bodies. Select only current/final `block_def_map` memos on
those exact keys, then reuse the existing local declaration/member/variant/macro
selection on each map. Repeat from newly discovered declaration keys to reach
nested item bodies; visit each block map once. Reading a selected persisted map
may decode it but must not collect it. Do not add another scope index, reconstruct
locations from syntax, follow read dependencies or collect untouched/stale maps.
Missing producer memos, including non-persisted body producers after restore,
remain a stated gap. Check actual body ownership, nested local items, shared
work, saved map decoding, repeated retirement, edits and no extra collection.

**Read-only inspection correction, 2026-11-21.** The body-local fixture exposed
a correctness issue in selection and diagnostic counts: `zalsa_mut()` bumps
Salsa's cancellation counter even with a sole handle. Its 8-bit rollover advances
the revision, so sufficiently many read-only inspections make untouched memos
fail current-value selection. Metadata inspection now obtains exclusive storage
directly when the database has one handle, without executing queries, mutating
memo values or changing cancellation/revision state. When other handles exist,
the existing cancellation/wait path remains. Dropping and spilling values keep
their existing mutation path. A regression repeats all four inspection APIs
512 times, checks exact selection, residency, revision and execution counts,
then verifies ordinary edit invalidation. Another check verifies cancellation
and waiting for a cloned handle before inspecting memos.

**Body-local retirement checkpoint, 2026-11-21.** Retirement now traverses
existing tracked-output records to current block maps and repeats the local
declaration, member, variant and macro selection there. Each block map is visited
once. No new source scan or ownership index is introduced.

The native fixture exercises cold/restored 1/4/16 families containing nested
functions, constants, statics, local trait/impl members, enum discriminants,
macro-generated functions and cfg-gated locals. Persisted block maps are spilled
before selection. Completed contexts retain zero tested body/projection values,
and each original body has only its explicit test reference left. Ordinary,
dependency and uncompleted-context objects retain database references. Repeated
retirement and same-ID re-entry preserve the facts without collecting block maps;
retirement itself collects neither bodies nor block maps. Separate checks skip
untouched/stale owners and the persisted maps whose non-persisted body producer
records are absent after restore, without rebuilding those producers.

All 91 focused Nextest tests passed across 13 binaries with `hotpath-alloc`,
including unchanged saved-query accounting checks. Formatting,
all-features/all-targets Clippy with warnings denied and diff checks passed.
No Cordial self-execution or full-program measurement ran. These tests establish
query residency and reference release, not retained bytes or RSS. Step 2 remains partial
for composite keys, absent/stale/provisional owner results, removed declarations,
missing producer records after restore and input/identity/header storage.

**Declarative expander key design, 2026-11-21.** Compiled declarative macro
rules already live in a regenerable query memo, but its implicit Salsa argument
interner hides the `(AstId<Macro>, defining Crate)` key from exact-key retirement.
Replace that implicit interner with a named interned key containing the same
two fields. Keep the existing `AstId::decl_macro_expander` API as a forwarding
method, with the compilation query on the named key. There is one compiled
value and one argument interner, not a wrapper memo or parallel ownership table.
The key keeps the existing interner's revision/reuse policy.

Derive that key from each local macro's existing interned location, filtering
out builtins and procedural macros. Select it from the same persisted local
macro table already used for retirement, including current block maps. Do not
follow imported or inherited namespaces, collect syntax or compile the macro to
select it. Collection already compiles local declarative definitions for their
rule styles; after restore, reconstructing a key whose non-persisted value was
not saved may allocate a small identity, but never recreates the compiled rules.
Input/identity/header storage remains a separate step. No Salsa API or dependency
representation change is needed for this RA query-key replacement.

Check actual expander memo residency in cold/restored 1/4/16 families, preserved
ordinary/shared/uncompleted values, repeat retirement without compilation,
same-ID re-entry with unchanged expansion facts, edited definitions, local and
generated macros, macro 2.0 and builtin exclusion. Query counts establish memo
residency, not retained bytes or RSS.

**Declarative expander retirement checkpoint, 2026-11-21.** Implemented the
named key above in RA, replacing the implicit argument interner and retaining
the existing forwarding API. The current local macro tables select compiled
declarative values at both crate and body-local scopes. Selection does not
compile missing rules, including after restore; ordinary, dependency and
uncompleted-context values remain resident.

The cold/restored 1/4/16 legacy-macro families include shadowed, nested, exported,
cfg-gated and generated definitions. Each completed context has zero resident
expander values after retirement. Repeated retirement compiles nothing;
same-ID re-entry recompiles exactly its nine definitions without recollecting
definition maps. The body-local families also retire their compiled macros.
Separate fixtures cover current versus replaced definitions, macro 2.0 expansion
before and after edits, builtin exclusion and missing non-persisted expanders
after restore. A shared-source-site check preserves distinct keys and cfg-based
transparency for two defining crates. The vendored incremental expectations
name the replacement query.

All 94 focused Nextest tests passed across 13 binaries with `hotpath-alloc`,
including unchanged saved-query accounting checks. Formatting,
all-features/all-targets Clippy with warnings denied and diff checks passed.
No Cordial self-execution or full-program measurement ran.

Step 2 remains partial for other composite keys, absent/stale/provisional owner
results, removed declarations, missing producer records and copied
input/identity/header storage. This checkpoint establishes expander memo
residency in fixtures, not retained bytes or process RSS.

**Item-tree composite key design, 2026-11-21.** Replace the implicit argument
interners for `file_item_tree_query(HirFileId, Crate)` and
`block_item_tree_query(BlockId, Crate)` with named interned keys containing
the same arguments. Preserve the query names, forwarding APIs, one memo per
argument combination and the default interner revision/reuse policy. No wrapper
memo, ownership index or Salsa API change is required.

Select keys from existing module origins, block maps and local macro-call
records, including empty expansions and macro calls in already-current
trait/impl item lists. A physical source file shared by two crates has distinct
keys: attribute lowering uses the requesting crate's cfg options. Imported
names do not authorize retirement of the dependency's tree. After restore,
selection may recreate a missing small key identity; it must not parse or lower
a tree to discover its key. Missing/stale block or associated-item owners retain
the previously documented skip behavior.

**Item-tree retirement checkpoint, 2026-11-21.** Implemented both named keys
and selection above. Cold/restored 1/4/16 families exercise a physical source
file shared with a dependency, inline and file modules, generated and empty
macro expansions, trait/impl macro calls and nested body-local block trees.
The fixture counts every resident item-tree memo, not just its selected keys.
After retirement, only ordinary, dependency and uncompleted-context tree
values remain. Repeated selection and retirement lower no trees. Re-entry
produces the original tree contents, and a separate restore/edit fixture proves
that selecting missing values does not lower them and edited source remains
visible after retirement.

All 96 distinct focused Nextest tests passed across 13 binaries with
`hotpath-alloc`. The first broad invocation passed 89; its seven apply fixtures
failed when their Cargo metadata subprocesses attempted a registry refresh.
Rerunning those seven with `CARGO_NET_OFFLINE=true` passed. Propagate that
environment setting to future fixture runs; a top-level `--offline` alone does
not configure subprocesses. No full-program measurement or Cordial
self-execution ran. Formatting, all-features/all-targets Clippy with warnings
denied and diff checks passed. These checks establish query residency and semantic reuse,
not retained bytes or RSS. The remaining composite families are listed below;
this checkpoint does not close them or the input/identity-storage work.

**Remaining composite-query inventory and design boundary, 2026-11-21.**
The source inventory of tracked queries in the patched RA crates and Cordial's
loader leaves these nine multi-argument queries after expanders and item trees:

| Query | Arguments beyond the database | Ownership issue |
| --- | --- | --- |
| `const_eval_query` | constant, substitutions, optional environment/crate | A local constant does not enumerate the substitutions already queried. |
| `anon_const_eval_query` | anonymous constant, substitutions, optional environment/crate | The same issue applies to generated anonymous constants. |
| `layout_of_ty_query` | type, environment/crate | There may be no local declaration at the outer type; nested types can reference local declarations. |
| `layout_of_adt_query` | ADT, substitutions, environment/crate | A dependency ADT can be instantiated with context-local arguments. |
| `monomorphized_mir_body_query` | body, substitutions, environment/crate | Body identity alone does not identify its instantiated memos. |
| `monomorphized_mir_body_for_closure_query` | closure, substitutions, environment/crate | Closure identity alone does not identify its instantiated memos. |
| `resolve_type_param_assoc_type_shorthand` | generic owner, parameter, associated name | Existing local owners do not enumerate the names already queried. |
| `specializes_query` | specializing impl, parent impl | Either impl can determine context ownership; imported parents must remain reusable. |
| `proc_macro_span` | definition-site AST ID, unit marker | This source-location query has no requesting crate; shared definition sites must remain reusable. |

Naming these keys alone does not make their existing combinations enumerable
from local definition maps. Tracked producer records do not own interned
argument combinations, and read dependencies are not ownership edges. Guessing
combinations would rerun analysis and still miss variants. Following all reads
would retire shared dependency work. A second reverse ownership table would
duplicate state and introduce another lifetime problem.

The next design review should decide how Salsa exposes *existing argument
combinations* for these families. A constrained extension of its existing memo
inspection path is preferable to another ownership table: inspect registered
query argument records without executing queries, decoding results or marking
interned slots current, then let RA classify their existing arguments. Restrict
inspection to named query families and report visited slots in controlled
fixtures. This still introduces enumeration cost; it must be weighed against
native owner-index support before implementing it. Ordinary public interner
`entries()` is unsuitable here because it marks enumerated slots current,
changing identity retention during an eviction operation. The earlier direct-key
design also rules out silently replacing exact selection with a global scan.

For either approach, define ownership for nested type/substitution/environment
references as well as outer owners, and distinguish context-dependent memos
from source-site-only shared memos. Prove multiple instantiations per owner,
shared generic owners with local arguments, direct calls outside tracked
producers, cold/restore/edit/re-entry and missing/stale owners. Keep the 1/4/16
families and shared/uncompleted residency assertions. Composite-key retirement
is **not complete** until that selection contract and its fixtures are implemented.

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
