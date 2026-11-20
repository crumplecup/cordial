# Hotpath allocation capability contract

Status: capability checkpoint, 2026-11-19. This records the locked dependency's
behavior, not completion of the [memory measurement plan](memory-measurement.md).

## Controlled evidence

`tests/hotpath_alloc_contract.rs` runs small fixtures in fresh test processes.
It never invokes Cordial or analyzes a project. Process isolation gives each
case fresh Hotpath counters and environment settings.

The dependency is `hotpath` 0.31.2. These are observed allocation-traffic results:

| Fixture | Evidence |
| --- | --- |
| Nested operations | A parent allocates 256 bytes and a child 128. Exclusive rows contain 256 and 128; inclusive rows contain 384 and 128. Each exclusive allocation count is one. |
| Retained return values | The nested operations return their buffers to the caller. Scope exit still records their allocation traffic; it does not report the outstanding bytes later held by each origin. |
| Grow and shrink | A 32-byte buffer grows to 64, then shrinks to 16. Hotpath reports 112 bytes and three allocation events, rather than a 16-byte live buffer or a net growth of minus 16. |
| Failed realloc | A local Hotpath allocator wraps a fault injector that allocates 48 bytes and rejects a 96-byte realloc. Hotpath records 48 bytes and one allocation; failure preserves the original block. |
| Failed alloc/zeroed alloc | Two rejected 24-byte requests still produce 48 bytes and two events. The dependency counts requests before checking their success. This prevents treating these totals as successful allocated bytes. |
| Cross-thread free | One worker allocates 192 bytes, another frees the returned buffer. The freeing thread records the free; its measured function allocates zero bytes. Allocation minus free on a thread cannot identify outstanding allocation origins. |
| Migrating async future | A future allocates 64 bytes, yields, moves to another thread, and allocates 96. Its function reports 160 bytes and two events. |
| Cancellation and teardown | A future allocates 80 bytes, then is cancelled from another thread. Its row records 80 bytes. A measured destructor allocates 40 bytes separately; even inclusive async totals omit destructor work outside the poll interval. |
| Short-lived workers and overflow | All 260 measured calls are reported. Exited workers retain thread rows, but fewer than 260 worker rows survive the 256-slot lifetime registry. Function traffic survives this registry limit; per-thread attribution does not. |
| Reserved thread names | A worker called `hp-fixture` is hidden from thread rows by default and appears with `HOTPATH_SHOW_INTERNAL_THREADS=1`. Its measured function still reports its 72 bytes. Application threads must avoid the `hp-` prefix. |
| Allocation-frame depth | More than the configured number of simultaneous guards causes a visible panic naming `HOTPATH_ALLOC_MAX_DEPTH`. It is a build-time limit, default 64, maximum 4096. |

Inspection of the locked dependency supplies additional boundaries that these
fixtures do not independently quantify: recorder work uses suspended allocation
tracking, thread counters begin on initialization, and human-formatted byte
fields lose precision above byte-sized display units. None of those boundaries
has been reconciled against a full-program run.

The source locations are the installed crate's `src/lib_on/functions/allocator.rs`,
`functions/alloc/{core,guard,report}.rs`, `futures/wrapper.rs`, `caller_stack.rs`,
`threads.rs`, and `build.rs`. The public
[allocator documentation](https://hotpath.rs/functions) also describes the
non-profiling allocator as a pass-through. It supplies no cheap always-on budget
meter through that configuration.

## Required Hotpath dependency work

The inspected 0.31.2 API has no process live/peak meter or retained allocation
origins. These requirements remain **blocked on a Hotpath extension**. Keep
`cap` for operational budget enforcement until its replacement passes the
following contract. Do not implement these counters or a pointer registry in
Cordial, and do not infer retained memory from function or thread traffic.

The extension must provide these behaviors within Hotpath:

1. Count only successful allocations. Failed `alloc`, `alloc_zeroed`, and
   `realloc` leave live bytes and origin records unchanged. Define realloc traffic
   separately from its net effect on live bytes; test both in-place and moved
   grow/shrink cases.
2. Expose exact integer process live bytes and peak live bytes. Define their
   interval, baseline, reset/snapshot semantics, and concurrency guarantees.
   Count successful allocations and matching frees before guard creation and
   after guard teardown, without requiring detailed function/thread recording.
   The cheap mode must work in ordinary builds with detailed profiling disabled.
3. Associate outstanding bytes with allocation origins when detailed recording
   is enabled. Origins survive scope/thread exit and retain identity across
   cross-thread frees and reallocations. Work outside a measured scope has an
   explicit unscoped origin. Define allocations predating capture and frees of
   unknown blocks; neither may corrupt accounting or silently become zero.
4. Export numeric counters and explicit accounting modes in JSON, alongside
   human-readable values. Include allocation successes/failures, live and peak
   snapshots, retained origins, pre-capture baseline, and excluded recorder work
   where supported. Missing capabilities must have a status, not an invented
   number. Origin is where allocation happened, not proof of current semantic
   ownership.
5. Export coverage counters for thread/origin registry overflow, dropped events,
   unscoped allocations, and depth/instrumentation limits. Preserve aggregate
   traffic when attribution fills up, and explicitly identify the lost portion.
   A bounded registry must never silently label a run fully attributed.
6. Specify metadata memory and runtime costs. Prevent allocator recursion, keep
   internal storage outside the tracked workload, define behavior when metadata
   allocation fails, and preserve thread safety during snapshots and teardown.
   Demonstrate behavior with small deterministic tests before discussing workload
   performance or choosing metadata limits.

Acceptance tests must cover pre-guard allocations, retained return values,
cross-thread free after origin exit, successful and failed realloc, overlapping
operations, async migration and cancellation, destructor allocations, registry
overflow, recorder exclusions, disabled detailed recording, and exact JSON
round trips. Snapshot sums must reconcile against the dependency's process
meter with baseline and exclusions explicit.

## Checkpoint and next work

Phase 1 is committed. The capability fixture establishes the existing traffic
path; it does not complete phase 2's live-memory requirements. No dependency
extension or `cap` removal has been made. Later lifecycle and ingestion work may
use the proven traffic path, carrying these limitations explicitly as allowed by
the plan. It must not claim retained-origin or complete RAM attribution.
