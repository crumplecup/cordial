# Memory profiling infrastructure

Hotpath is the allocation measurement authority. Cordial owns capture lifetimes,
expected process/test identities, storage and presentation. The implementation
is validated with controlled unit fixtures. Full-program measurement, including
`elicit_doc`, awaits maintainer review and a separate target discussion.

The global `hotpath::CountingAllocator` provides the live requested-byte meter
used by budget control in every build. Detailed recording is optional:
`hotpath` enables timing and `hotpath-alloc` additionally enables allocation
traffic, retained origins and exact allocation JSON. The JSON data types are
available to import existing reports even in a build without recording.

## Ownership and capture

The CLI starts `ProcessProfile` before tracing or argument parsing. Configuration
resolves its final store without restarting recording. Ordinary completion
finishes after workload teardown; error, unwind, watchdog, resume and no-room
exits retain their distinct completion reason. A watchdog or process exit can
bypass stack teardown. Uncatchable termination leaves a running/incomplete
manifest rather than a fabricated partial report.

Tests can bind `let _profile = cordial::profile_test()?;` before fixture setup.
Keep the binding alive until after fixture teardown. This guard owns a helper
thread that finalizes the shared capture before test-thread TLS destruction.
Legacy `init_tracing` ownership starts at `first_init_tracing`, explicitly
leaving earlier fixture setup unverified. The test harness and later TLS drops
remain boundaries. A process supports one capture lifetime, not restartable
recording sessions.

Expected isolation and workflow children are registered before spawn. Each has
a unique capture id, parent link, restart number, producer features and raw
path. Workflow states and repeats survive cold-store deletion and resume.
Library workflow callers without a driver profile use the same child tickets
and lifecycle reader. Spawn failures and missing, crashed or truncated child
reports remain visible. Nextest's existing event stream supplies the expected
test inventory, including tests which never initialize profiling.

Raw reports and manifests live under the configured store's `perf/raw/profiles`
area, with unique invocation/process directories. Library workflows without a
driver owner retain their inventory beneath the workflow work directory,
in a distinct project-derived directory outside the disposable project store.
`RunRecord::memory` stores imported Hotpath allocation DTOs and provenance while
retaining the raw report path.
Existing records without these fields remain readable and explicitly lack a
capture inventory.

## Quantities and limits

| Quantity | Meaning and interval |
| --- | --- |
| Function bytes/count | Exact allocation traffic in the report collection window; count and bytes are distinct. Exclusive and inclusive modes cannot be mixed. Inclusive rows cannot be summed. |
| Thread traffic | Allocated and freed requested bytes with capture-local thread id, including exited threads. The freeing thread does not establish retained ownership. |
| Retained origin | Ending requested bytes and live allocation count charged to the allocation origin, even after its scope exits or another thread frees it. |
| Heap live/peak | Requested bytes and allocator-lifetime peak, with zero lifetime baseline and a separate capture-start snapshot. Individual process/origin peaks and ending live values are never summed. |
| Coverage | Allocator-lifetime overflow, unscoped, TLS, thread/depth and other loss counters. These are not interchangeable with the function collection interval. |
| OS envelope | Unavailable in the perf allocation capture. Existing OS pressure telemetry remains a separate budget backstop. Native allocations, mmap, stacks, shared pages, allocator overhead/retention and private headers are outside requested sizes. RSS minus heap is not an ownership partition. |

`read_memory` imports workflows and test captures using the same lifecycle
reader. `profiled_from` attaches both timing summaries and that complete process
inventory. Zero, disabled, unsupported, missing, malformed, truncated and
incomplete data remain distinct. An exact section forbids fallback to rounded
legacy strings. Original exact integers, thread ids, origin ids, capture times,
completion reasons and feature/accounting metadata survive record storage.

Reports display every producer's status and coverage gaps, plus all ten regions
of the boundary inventory. The `top` display limit never truncates stored exact
data. Repeats and states remain separate; exact data is not merged by thread or
function display name. Comparison requires matching producer versions,
features, state/repeat/restart, completion, interval boundary and accounting
mode, with matching operation inventories. Missing, lossy or incompatible
captures withhold byte deltas. Memory completeness is independent of the timing
verdict; unverified boundaries always prevent a full attribution claim.

The current inventory explicitly leaves full-pipeline cache behavior, opaque
dependency internals, cross-spawn linkage, delayed worker/TLS destruction,
external helper heaps and native memory unverified. Controlled fixtures prove
the infrastructure's measured boundaries, not complete program coverage.

## Controlled validation

`tests/hotpath_alloc_contract.rs` exercises nested/retained allocations,
cross-thread frees, reallocations, failure, registry overflow and async polling.
`tests/hotpath_lifecycle.rs` covers ordinary/error/unwind/controlled/uncatchable
exits, resume and missing children. `tests/hotpath_coverage.rs` reads a small
shared-loader IR fixture; `tests/hotpath_workers.rs` verifies named worker
origins. `tests/perf_memory.rs` proves actual allocation capture through import,
record storage and display, exact integers above 2^53, mode validation, missing
and malformed inventories, and compatibility/loss handling in comparison.
Workflow programs in these tests are shell stand-ins or the unit-test executable.

The phase gate is `cargo clippy --all-features --all-targets -- -D warnings`.
Do not execute Cordial or a real-program benchmark as part of this infrastructure
validation. The ordered plan and checkpoint evidence are in
[the memory measurement plan](planning/memory-measurement.md).
