# Memory profiling infrastructure

Cordial uses published Hotpath 0.31.2 unchanged for allocation measurement.
Cordial owns labels, capture lifetimes, expected identities, storage and
presentation. Controlled unit fixtures validate the integration.

`hotpath` enables timing. `hotpath-alloc` also enables upstream allocation traffic
and declares `hotpath::CountingAllocator` once in `src/profiling.rs`. Disabled and
timing-only builds need no allocation meter. Operational pressure uses process
RSS in every build; freed objects can leave resident allocator pages.

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
`RunRecord::memory` stores upstream report sections and provenance while
retaining the raw report path.
Existing records without these fields remain readable and explicitly lack a
capture inventory.

## Workload history

Use the existing project store and perf workflow collector. Working state is
reused under `perf/work`; immutable records under `perf/records` link to raw
reports. Do not create a new store for each manual run. Use `perf list`,
`perf compare` and explicit `perf baseline` selection for history.

The previous unlimited-budget `elicit_doc` capture used the rejected fork; its
retained metrics are not an upstream baseline. Discuss readiness before another
full-program run, with both memory controls zero and fixed target/configuration.

## Quantities and limits

| Quantity | Evidence and limits |
| --- | --- |
| Function traffic | `functions_alloc.data`, with `alloc-bytes` or `alloc-count`. Exclusive accounting is the default; cumulative rows overlap. Formatted byte strings retain upstream precision and may be approximate. |
| Thread traffic | Upstream allocated/freed totals. Frees belong to the freeing thread and do not identify object ownership. |
| Process RAM | Upstream RSS and OS high-water fields, plus existing OS pressure observations. These are separate from allocation traffic. |
| Retained ownership and simultaneous heap composition | Unavailable through the inspected public upstream API. Traffic and per-call maxima do not establish them. |
| Coverage | Instrumented Cordial boundaries and their fixture evidence. Opaque dependency interiors, unscoped work, other execution threads, native allocations, mmap, stacks and profiler overhead remain limits. |

Missing, disabled, unsupported, malformed, truncated and incomplete data never
become zero. Display limits do not alter stored raw evidence. Comparisons require
compatible producer/configuration metadata and operation inventories; incompatible
or truncated captures withhold traffic deltas. Traffic reductions do not prove a
lower process peak. Historical fork payloads remain readable with provenance,
excluded from upstream traffic comparisons.

Upstream environment settings override guard output paths, formats and limits.
Manifests record overrides; redirected or non-JSON output can leave the expected
capture unavailable. Cordial does not patch Hotpath to suppress these settings.

## Controlled validation

`hotpath_lifecycle` covers setup/teardown, exits, resume and child failures.
`hotpath_workers` proves execution-thread scopes. `hotpath_coverage` uses a small
shared-loader fixture. `perf_memory`, `perf_usage` and `hotpath_json` exercise
actual upstream output, legacy evidence, precision, modes, storage and comparison.
Unit subprocesses execute fixtures, never Cordial on itself.

Gate each checkpoint with focused tests, `cargo fmt --check`,
`cargo clippy --all-features --all-targets -- -D warnings` and `git diff --check`,
then commit. No Cordial self-execution is authorized for this work. See the
[current plan](planning/memory-reduction.md) and
[consumer contract](planning/hotpath-allocation-contract.md).
