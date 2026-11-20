# Hotpath allocation capability contract

Status: phase 2 implemented, 2026-11-19. This establishes the dependency API and
controlled evidence. Process lifecycle, operation coverage, ingestion and
full-program attribution remain later work in the
[memory measurement plan](memory-measurement.md).

## Supported path

The dependency is the published `hotpath` 0.31.2 source with the local extension
in `vendor/hotpath.patch`; provenance and reconstruction instructions are in
`vendor/hotpath/PROVENANCE.md`. This capability is a local dependency change,
not a claim about an upstream release.

Cordial declares one `hotpath::CountingAllocator` in `src/profiling.rs` and reads
its `live_bytes()` for budget enforcement in every build. `cap` is removed.
Cordial's `hotpath-alloc` enables Hotpath's traffic and retained-origin features.
The same guard construction point supplies the allocator's `heap_snapshot()`
through `HotpathGuardBuilder::heap_provider`. All counting, allocation metadata,
origin attribution, snapshots and coverage counters belong to Hotpath. Cordial
contains no replacement allocator profiler or pointer registry.

## Quantities and intervals

| API/data | Meaning |
| --- | --- |
| `CountingAllocator::live_bytes()` | Outstanding successful requested bytes through this allocator instance. Starts at zero on construction; includes pre/post-guard, unscoped and recorder allocations. |
| `peak_live_bytes()` | Highest simultaneous requested live bytes over that allocator's lifetime up to the read. No reset or interval-peak subtraction. |
| `heap_snapshot()` | A coherent registry snapshot of live/peak requested bytes, live block count, traffic, failures, retained origins and overhead at one lock point. Available with `hotpath-retained`. |
| `HeapSnapshot::origins` | Exclusive outstanding requested bytes/blocks by allocation origin, including explicit unscoped, recorder, unavailable-TLS and origin-overflow rows. Rows sum to snapshot live bytes/blocks. Origin is allocation location, not semantic ownership. |
| Function traffic | Successful allocation bytes/count within measured frames. Exclusive by default; `HOTPATH_ALLOC_CUMULATIVE=1` makes function traffic inclusive. Never add inclusive function rows. |
| Thread traffic | Allocation/free bytes on the executing thread since thread tracking initialization. Allocation minus free on a thread does not identify retained origins. |
| Report `heap_baseline` / `heap` | Before guard initialization / after the `before_shutdown` callback, before worker draining and report construction. Peaks remain lifetime peaks. These snapshots do not imply all workload teardown has occurred. |
| Report coverage | Independent process-lifetime counters read during report construction. Function rows cover the guard collection window; heap has its own sample point. Coverage fields overlap and are not a disjoint partition. |

Successful realloc counts a full old-block free and new-size allocation in byte
traffic, and one successful realloc event. Requested live bytes change only by
the size difference, even if the inner allocator moves the block. Retained bytes
stay with the original allocation origin; function traffic belongs to the
resizing operation. `successful_allocations` includes ordinary and zeroed allocs
and excludes realloc; `deallocations` counts explicit frees. Failed requests
change neither live bytes nor origins and increment the appropriate failure
counter in retained mode.

Cheap live/peak reads are independent atomic loads, rather than a transactional
pair. Detailed-mode updates publish these atomics under the registry lock, so
its lifetime peak agrees with the registry after allocator calls quiesce. The
registry snapshot's rows/totals are coherent even with workers allocating and
freeing. It does not stop the workload. Snapshot storage is recorder work;
when using the global allocator, its temporary buffer can appear at the sample
point and its returned data can remain live afterward.

Only allocations through this instance are valid inputs to its free/realloc.
The global allocator tracks from process initialization, so a block predating a
guard is already tagged and appears in its baseline. There is no runtime switch
that starts tagging existing unknown blocks. Passing a foreign pointer or wrong
layout violates `GlobalAlloc`'s contract; there is no invented zero-sized origin
for such a block.

Native allocations bypassing Rust's global allocator, mmap, stacks, allocator
retention/overhead and temporary backing storage inside realloc remain outside
requested heap. OS measurements remain the envelope and budget backstop; their
subtraction from requested heap does not make an exact memory partition.

## Exact reports and coverage

`JsonReport::allocation` is a versioned numeric section alongside upstream human
fields. It retains function identity, byte and count values independently of
byte/count display selection and row truncation. Thread rows also retain exact
integers. Unsupported values remain null. Timing-only reports explicitly mark
allocation recording disabled; without a heap provider, heap status is
unavailable. Neither case becomes zero. Numeric serialization preserves u64
values above 2^53; consumers must use integer parsing.

The coverage object exposes recorder/unscoped traffic, traffic before thread
tracking, hidden threads, TLS unavailability, thread overflow, depth-limit hits,
cross-thread guards, unwound future polls, inactive recording and dropped
measurements. Heap snapshots expose origin capacity, registered origins, overflow
allocation bytes/count and the outstanding overflow bucket. Traffic and retained
origins remain distinct when an attribution limit is hit.

The upstream thread registry holds 256 lifetime slots, including exited threads.
Overflow loses per-thread attribution while aggregate traffic survives. The
reserved `hp-` thread-name prefix hides threads unless
`HOTPATH_SHOW_INTERNAL_THREADS=1`; this also hides application threads with that
prefix. Function/origin attribution still works. Allocation-frame depth is
build-time configured, default 64, maximum 4096. Exceeding it panics visibly and
increments coverage. Synchronous guards must not cross threads or span migrating
polls. An unwinding poll restores its frame and marks incomplete traffic through
`unwound_polls`; do not treat its function traffic as complete. Cancellation's
destructor work occurs outside the poll and needs its own measured scope.

## Metadata, bounds and concurrency

`hotpath-live` alone adds two atomic integers per allocator instance. It allocates
no metadata, uses no lock or TLS, and does not change user layouts. Detailed
recording is optional for budget enforcement.

`hotpath-retained` additionally embeds a fixed registry of 1024 function origins
and four reserved buckets in each allocator. No per-block pointer map or dynamic
metadata allocation exists. Slots last for the allocator's lifetime, including
after scopes/threads exit. `with_origin_capacity(inner, capacity)` can lower the
logical capacity for controlled overflow tests; storage remains fixed. Overflow
uses an explicit aggregate origin and never loses outstanding byte/block totals.

Each block has a private `usize` origin id preceding its user region, with padding
to preserve requested alignment. The prefix offset is
`size_of::<usize>().next_multiple_of(user_alignment)`. Thus highly aligned blocks
can have much larger padding than the tag itself. `live_header_bytes` reports
current prefix/padding bytes separately; `registry_bytes` reports the embedded
registry size. Neither is hidden inside requested bytes. User data stays zeroed
for zeroed allocation; realloc preserves contents and the origin tag.

A spinlock serializes bounded origin lookup and registry updates. Lookup is
linear in registered origins and compares static names; free/realloc use the id
directly. No inner allocator call, TLS lookup, formatting, dynamic allocation or
user callback occurs under that lock. Snapshots reserve scratch capacity before
locking, copy bounded rows while locked and format owned names after unlocking.
Detailed mode has these costs on every allocation, including outside a guard;
unit evidence does not establish its workload performance or tune the capacity.

Fixed metadata cannot fail to allocate dynamically. Layout extension can fail,
or the inner allocator can reject an allocation/realloc: the request returns
null, failure counters increase, and an existing block stays valid. Snapshot and
report vectors/strings still use ordinary Rust allocations and can fail like
other report construction. Reporting failure/completion across process exits
belongs to the later lifecycle work. Allocator callbacks use fallible TLS access
and an explicit unavailable bucket rather than requiring guard lifetimes during
TLS teardown.

## Controlled evidence

All fixtures are under `tests/`, with small buffers. Tests never invoke Cordial,
analyze a project, benchmark a workload or measure a full program.

| Fixture | Evidence |
| --- | --- |
| `hotpath_heap` | Six local-allocator tests: successful/zeroed allocs, deterministic in-place/moved grow/shrink realloc preserving contents and origin, failures preserving live state, independent instances, overlapping workers and cross-thread free after thread exit. They also run in live-only builds. |
| `hotpath_alloc_contract` | Fresh test processes establish exclusive/inclusive/count modes, exact 12345-byte capture, 256-slot thread overflow, hidden internal-name threads, depth limits, guard-independent live meter, migrating async polls, cancellation/destructor traffic and poll-unwind coverage. Only the unit-test executable launches. |
| `hotpath_retained` | Eight local-allocator tests establish nested returned blocks, stable origin identity across thread exit/resize/free, bounded origin overflow, 4096-byte alignment/zeroing, concurrent snapshots, migration/cancellation, unwind stack restoration and pre-capture/recorder retention in JSON independent of row truncation. Every heap snapshot reconciles rows and byte traffic. |
| `hotpath_json` | Exact integer/null round trips without large allocation, plus explicit disabled recording in timing-only builds. |
| `live_bytes`, `hotpath_allocator`, `salsa_eviction` | Public meter behavior, shared allocator/guard wiring and preserved cache pressure/eviction primitives. |

Phase 2 supplies the capability contract and dependency implementation. It does
not claim complete program coverage. Phase 3 must begin/end capture at the proper
process lifetime boundaries and identify partial/missing processes. Phases 4–5
must cover the operation inventory and preserve these quantities/statuses through
the existing perf ingestion and presentation path. Full-program measurement,
including the proposed `elicit_doc` target, remains deferred for discussion.
