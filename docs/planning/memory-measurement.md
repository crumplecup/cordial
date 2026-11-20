# Memory measurement through Hotpath allocation profiling

Status: **Infrastructure complete for maintainer review, 2026-11-20.**
Phases 1–5 are implemented and gated. Phase 5 unifies import, storage,
rendering and comparison through the existing perf path.
Hotpath supplies the always-on live/peak meter, retained allocation origins,
exact JSON and explicit coverage counters; `cap` and the parallel diagnostics
are removed. No full-program measurement has been authorized or performed.
The remaining full-program boundaries are explicit in every report.

## Objective and decisions

Use `hotpath-alloc` as the allocation measurement path throughout Cordial.
Remove the parallel memory ledger and tracing-span memory profiler. Make a
profile explain allocation activity throughout a run, identify which allocation
origins still account for live heap, and expose every coverage limitation.

Motivation and constraint: Cordial currently consumes too much RAM to run `open`
or `submit` on its own repository. The requested work is to establish complete
memory measurement through Hotpath and remove the parallel homebrewed efforts.

Validation for this effort is through unit tests with small, controlled fixtures.
Do not benchmark Cordial or profile full programs during this phase. Put the
infrastructure in place and demonstrate it through those tests for maintainer
review first. After that, discuss using `elicit_doc` as a real-program target;
that measurement is deferred and is not authorized by this plan.

This is the implementation plan for memory measurement. It complements
[performance metrics](performance-metrics.md) and narrows the allocation claims
in the [Hotpath etiquette plan](hotpath-etiquette.md). Building the general
etiquette is not a prerequisite for measuring this program.

Decisions:

- Hotpath owns allocation counting and attribution. Cordial owns lifecycle,
  operation labels, report storage, ingestion, and presentation.
- A missing Hotpath capability is addressed through Hotpath, with a tested
  dependency change if necessary. Do not introduce another allocator profiler,
  tracing layer, pointer registry, or before/after attribution system in Cordial.
- Remove the custom diagnostic systems first. Preserve eviction, persistence,
  pressure control, worker reuse, and resumable execution unless a particular
  piece exists solely to support those diagnostics.
- Keep operating-system memory readings as the process envelope and budget
  backstop. Label their relationship to heap data explicitly; they cannot
  attribute heap allocations to functions.
- Preserve budget enforcement in builds without detailed profiling. Consolidate
  its live-byte source too; do not silently require a full profile to enforce a
  memory limit.

## What the current tree actually does

Assessment baseline: `3709e09b`, with existing uncommitted changes to
`src/memory_ledger.rs` and `src/loader/database/eviction.rs`, and untracked
`src/span_memory.rs`. Preserve that distinction when reviewing the removal.

| Path | Current behavior | Planned disposition |
| --- | --- | --- |
| `src/profiling.rs` | One global allocator: `cap::Cap` wraps Hotpath's allocator with `hotpath-alloc`, otherwise `System`. `cap` supplies `live_bytes()`. One guard builder enables unlimited report rows. | Keep the central integration point; make Hotpath the counter authority after the capability work below. |
| `src/memory_ledger.rs` | Drops each query's values, waits for process memory to settle, and assigns the difference to that query. Creates one `lgNNN` thread per query so Hotpath can also report frees on those threads. | Remove the module, public types/function, settling loop, and artificial threads. |
| `src/loader/database/eviction.rs` and callers | `CORDIAL_MEMORY_LEDGER`, occurrence selectors, option checks, and ledger logging at eviction/stage boundaries. The diagnostic also reads file texts to count their lengths. | Remove diagnostic hooks and selectors. Keep actual cache management. Replace any operational dependency on `settled()` with the appropriate direct metric, without keeping its attribution algorithm. |
| `src/span_memory.rs` | Unwired tracing subscriber that subtracts process-wide live bytes at span boundaries, including a child-subtraction calculation. | Remove; do not wire it into the subscriber. Parallel activity prevents that subtraction from proving span ownership. |
| `src/process_memory.rs`, `src/memory_limit.rs` | OS memory sampling, footprint logging, budget/pressure decisions, and use of `live_bytes()`. | Retain operational behavior; review diagnostic growth claims and route allocation data through Hotpath. |
| `src/perf/usage.rs` and profile reader | Import function allocation totals and thread allocation/free totals. Missing/unparseable fields can become zero or disappear; reports merge by name. | Extend the existing reader and run record, preserving identity and completeness. |
| `src/perf/workflow.rs` | Reads child gate ledgers and run metrics, but does not import child Hotpath reports. | Connect it to the same profile reader used for test profiles. |

The ledger's current eviction log also mixes a resident-memory reading before
the operation with the newly changed live-heap reading after it. Removing the
ledger must remove that unit/source mismatch, rather than reproduce it.

The historical `alloc_metrics` experiment described in `ra-persistence.md` is
already removed. Keep its history; do not revive it as the replacement.

## Measurement contract

These quantities answer different questions and must remain distinct:

| Quantity | Meaning and required treatment |
| --- | --- |
| Allocation bytes/count | Traffic produced by an operation, including temporary objects that have already been freed. |
| Free bytes/count | Traffic on the freeing thread; a free may happen on a different thread and long after allocation. |
| Live heap at a point | Successful allocation sizes still outstanding, with a defined baseline and complete allocation/free tracking. |
| Peak live heap | Maximum simultaneous live heap over a stated interval. Summing per-operation or per-process peaks does not give a run peak. |
| Live heap by allocation origin | Outstanding allocations associated with where they were allocated, even after the allocating scope or thread exits. This does not prove which current object semantically owns a shared allocation. |
| Resident/process memory | OS view including effects outside Rust's global allocator. It is not interchangeable with requested heap sizes. |

Function totals need an explicit exclusive/inclusive mode. Use exclusive bytes
for accounting and inclusive views for navigation; do not add inclusive rows
together. A worker's allocations belong to its measured worker operation, not
automatically to the parent function that scheduled it.

### Capability boundary in the locked dependency

Inspection of the unpatched `hotpath` **0.31.2** source established:

- Function allocation frames track allocated bytes and count. Deallocation
  updates thread totals, not the function that originally allocated the block.
- Function attribution is exclusive by default; `HOTPATH_ALLOC_CUMULATIVE`
  propagates child allocations to parents.
- Thread allocation tracking begins when initialized. The registry has 256
  lifetime slots, preserves exited threads, and counts overflow traffic in
  aggregate totals. Per-thread attribution is lost for overflow threads.
- Hotpath suspends counting for recorder work and can hide its internal threads.
- Function allocation nesting has a build-time depth limit. Async measurement
  has a separate mechanism; a synchronous guard must not span a migrating future.
- The inspected interface does not provide retained bytes by allocation origin
  or a live/peak API that can replace Cordial's `cap` meter. Human-formatted report
  byte strings also lose precision in the current reader.

Controlled fixtures now verify the traffic behavior, including migrating async
functions and cancellation. See the [capability contract](hotpath-allocation-contract.md)
for observed results, remaining boundaries, and the required dependency work.

The local dependency patch now supplies lifetime live/peak requested bytes,
retained allocation origins, successful-request traffic, exact JSON and coverage
counters. Cordial's budget reads Hotpath directly; `cap` has been removed. The
[capability contract](hotpath-allocation-contract.md) defines intervals, baseline,
origin identity, bounds, metadata overhead, failure and concurrency behavior.

The dependency can now identify outstanding requested heap by allocation origin.
Phases 3–5 now own process lifetimes, expose operation coverage and import the
exact data through the existing perf path. Full-program coverage remains
unverified and visible in every capture. Do not relabel thread
allocation-minus-free as retained ownership.

## Coverage inventory

Every row needs a measured path, a deliberate boundary, and a verification case.
“No blind spots” means gaps are detected and exposed, then closed where possible;
an empty report is never accepted as evidence of zero memory use.

| Region | Coverage work |
| --- | --- |
| Entry and setup | Start profiling before tracing, CLI parsing, configuration, workspace loading, and runtime construction. Establish what predates Rust entry and how the baseline is represented. |
| Pipeline | Cover load, semantic initialization/definition maps, priming, shared IR collection/enrichment, evidence, cfg contexts, projection, assessors, reporting, apply, and output serialization. Include both cache hits and recomputation. |
| Dependency work | Allocations through the process allocator are counted, but opaque dependency work may be charged to a Cordial boundary. Add targeted Hotpath scopes at important vendored RA/Salsa query/build boundaries when that attribution is too coarse. |
| Concurrent work | Cover Tokio runtime and blocking pool, Rayon/RA workers, Salsa work, syntax background drops, watchdog, and helper threads. Include short-lived and exited threads. Verify the actual thread inventory; names alone do not prove coverage. |
| Async work | Use Hotpath's supported poll-aware instrumentation for async bodies. Check yielding, migration, cancellation, and allocations in blocking jobs. |
| Eviction and shutdown | Include normal eviction, spill/restore, result-store flush, database destruction, delayed background frees, error unwinding, and normal teardown before finishing the report. Do not measure TLS destructors with unsafe guard lifetimes. |
| Isolation | Profile the driver and every resumed child. Currently the driver calls `process::exit` before the guard is created. Record parent/child role and restart sequence. |
| Other subprocesses | Inventory Cargo/rustc/proc-macro and other helpers. Distinguish Cordial children that can emit Hotpath reports from external programs whose heap is outside this allocator. Include their OS envelope where the run includes them. |
| Test processes | Start the per-test profile before measured fixtures/setup. Record tests that never initialize profiling and missing or failed reports. Keep test/process identity before aggregation. |
| Recorder and native memory | Expose excluded recorder work and the residual OS envelope: native allocations bypassing Rust's allocator, mmap, stacks, shared/file-backed pages, and allocator retention/overhead. Do not manufacture an exact partition by subtracting live heap from RSS. |

## Ordered implementation slices

### 1. Remove the parallel diagnostics

- Delete `span_memory.rs` and `memory_ledger.rs`; remove ledger exports, options,
  environment handling, stage hooks, and diagnostic-only tests and metrics.
- Review `tests/memory_ledger.rs`, `tests/salsa_eviction.rs`, and vendored Salsa
  additions by caller. Keep the primitives used for real cache eviction and
  tests of that behavior; remove facilities used only for diagnostic itemization.
  Keep `vendor/salsa.patch` consistent with retained vendor changes.
- Review `process_memory::Watch` and phase memory events. Keep only OS-envelope
  sampling and operational diagnostics that are still needed, and carry that
  context into the same profile/run record. Remove duplicate allocation reports
  and diagnostic-only watcher threads where Hotpath already supplies the data.
- Keep before/after operational telemetry honest about its metric and source.
  No artificial query-drop threads or “settled delta means owner” replacement.
- Search the working tree for remaining live references and update active docs.

Exit evidence: the custom attribution paths are gone; pressure/eviction behavior
still has its required APIs and focused behavioral checks.

### 2. Establish the Hotpath capability contract

- Build small controlled fixtures using the locked dependency to establish
  exclusive/inclusive accounting, cross-thread frees, realloc behavior, thread
  overflow, recorder exclusions, depth limits, and async behavior.
- Determine the supported Hotpath path for exact machine-readable counters,
  process live/peak bytes, and outstanding bytes by allocation origin. Where the
  locked dependency lacks them, specify and test a Hotpath change rather than
  implementing a Cordial-owned profiler.
- Require allocation-origin tracking to survive scope/thread exit, cross-thread
  frees, and reallocations. Define allocation failure handling and overhead.
- Provide a cheap live-byte mode suitable for normal budget enforcement without
  detailed profile collection. Migrate `live_bytes()` callers and remove `cap`
  only once this preserves behavior with and without detailed profiling.

Exit evidence: a documented supported API and fixtures prove each claimed
quantity. If retained-origin measurement is unavailable, mark that requirement
blocked on Hotpath; available allocation-traffic work can continue with that
limitation visible. Do not declare RAM attribution complete.

### 3. Cover each process lifetime and collect its report

- Retain one guard construction point, with an explicit owner for each process.
  Begin early; stop after the workload and its normal teardown have completed.
  Resolve store configuration without delaying initial capture.
- Write unique raw reports under the configured store, with a run manifest
  linking process identity, role, parent, restart, state/repeat/test, enabled
  features, Hotpath version, accounting mode, and report completion status.
  An explicitly configured output path must not be overwritten by sibling runs.
- Use the existing watchdog finish path consistently. Controlled exits must
  finish once; uncatchable termination must leave a visibly incomplete run.
  Check whether checkpointed Hotpath reports can preserve useful partial data.
- Verify features in every executable being measured. A timing-only build must
  report allocation profiling as disabled, rather than produce zero allocations.

Exit evidence: unit tests cover ordinary, error, watchdog, and resumed-process
lifecycle cases and report linkage; missing/partial children are visible in the
run record using controlled report fixtures.

### 4. Close operation and concurrency gaps

- Work through the coverage inventory using existing `#[hotpath::measure]` and
  supported block/future scopes. Measure dependency boundaries narrowly where
  needed to distinguish meaningful allocation origins.
- Use Hotpath context propagation if supported when relating worker tasks to a
  stage. Preserve thread and operation views even without parent linkage.
- Make registry overflow, unscoped activity, excluded work, and instrumentation
  limits visible. Zero overflow is required for a claim of full thread attribution.
- Strengthen coverage checks beyond the existing scan of functions that already
  have debug/info tracing. It misses uninstrumented entry points and closures;
  the allocator scan also assumes an older feature-gated declaration shape.
  Static policy work must use the shared loader/IR, per repository rules.

Exit evidence: each inventory row has unit-test evidence or an explicitly
documented unverified boundary, and each remaining boundary appears in the
report. Full-program coverage remains unverified until the later target review.

### 5. Read everything through the existing perf path

- Extend `ProfileUsage`, `RunRecord`, rendering, and comparison; use that reader
  for both workflow children and test profiles. Keep the raw Hotpath report as
  the source of truth. Do not add another collection subsystem.
- Distinguish zero, disabled, unsupported, missing, malformed, truncated, and
  incomplete data. Validate profiling mode so allocation-count reports are not
  silently parsed as byte reports. Preserve exact numeric fields when available.
- Preserve process/thread identity and time/phase provenance before producing
  summaries. Separate repeats and cold/warm/incremental states. Sum traffic only
  over disjoint records; derive simultaneous peaks from compatible time data,
  never by summing individual peaks or ending live-byte values.
- Present allocation traffic, current retained origins, peak, OS envelope, and
  coverage status with their definitions. Keep unsupported quantities visible.

Exit evidence: fixtures with known allocations survive capture, import, storage,
and display; missing child reports and overflow cannot yield a complete result.

## Validation and completion

Unit tests during implementation should cover controlled nested allocations,
objects retained past scope exit, cross-thread frees, grow/shrink/failing realloc,
short-lived threads and registry overflow, async migration/cancellation, normal
and error teardown, watchdog termination, and parent/child report import. Use
meaningful controlled fixtures, rather than assertions about source spelling.
Keep the test cases small and independent of Cordial's own analysis workload or
any other full program. Follow the repository's `tests/` placement convention.

Reconcile controlled allocations with imported Hotpath totals using matching
intervals and accounting modes. Test that unscoped activity, overflow, exclusions,
missing reports, and unsupported quantities remain visible. Use numeric fixtures
to verify precision and aggregation instead of obtaining benchmark measurements.

Infrastructure completion requires all of the following:

- No active custom span-delta or destructive ledger attribution remains.
- Hotpath is the allocation counter authority, including the live-byte source
  used by budget control; detailed profiling remains optional.
- Every owned process and inventory region is measured or has an explicit,
  evidenced boundary. Allocation-origin retention is proven before claiming to
  explain held RAM; allocation traffic alone does not meet that claim.
- The existing perf workflow retains the data and refuses to call incomplete
  coverage complete. Unit tests demonstrate capture, ingestion, storage, and
  presentation with known allocations and report fixtures.

Present the infrastructure and unit-test evidence for maintainer review before
starting full-program measurement. `elicit_doc` is the proposed later target,
subject to discussion of how to use it. Neither a Cordial benchmark nor a
real-program run is an acceptance requirement for this infrastructure phase.

The maintainer directs us to **not run Cordial**, to validate with controlled
unit tests, and to commit at completed phase points using
`cargo clippy --all-features --all-targets -- -D warnings` as the gate.
Do not run the usual Cordial pre-commit gate during this effort.

## Implementation evidence

### Phase 1

Removed the destructive ledger module, artificial query-drop threads, settling
loop, stage/environment/option hooks, public exports, and diagnostic-only tests.
Removed the unwired, untracked span profiler. Kept Salsa's query-value APIs because
both pressure eviction and scaffolding eviction use them operationally; their
controlled behavior tests now live in `tests/salsa_eviction.rs`. No vendor API
or patch changes were needed. OS sampling remains envelope telemetry and no
longer emits a duplicate live-heap figure in its phase event. Eviction takes
direct RSS samples before and after without claiming allocation ownership.

Validation: `cargo test --all-features --test salsa_eviction -- --test-threads=1`
passes all four controlled cache tests.
`cargo clippy --all-features --all-targets -- -D warnings` passes. No Cordial
command or full-program measurement was run.

### Phase 2 capability checkpoint

Added `tests/hotpath_alloc_contract.rs` and documented its observed results in
the [Hotpath capability contract](hotpath-allocation-contract.md). The contract
specifies the missing live/peak, retained-origin, cheap budget-meter, exact JSON,
and coverage APIs required from Hotpath. At that checkpoint, `cap` remained to
preserve the operational budget meter. RAM attribution was incomplete and phases
3–5 were pending; the next checkpoint below supersedes the meter status.

Validation: the controlled allocation contract passes all six subprocess
scenarios, and the four Salsa eviction tests still pass.
`cargo clippy --all-features --all-targets -- -D warnings`, formatting, and
`git diff --check` pass. No Cordial command or full-program measurement was run.

### Phase 2 live-meter checkpoint

Vendored the published Hotpath 0.31.2 runtime with provenance, upstream license,
and a reproducible patch. The extension adds `hotpath-live`, independent of
function profiling, and uses one allocator implementation in every build.
Its live and lifetime-peak counters need two atomic integers per allocator
instance, no metadata allocation or pointer registry. Failed requests now count
zero traffic; realloc uses a size difference for live bytes and full replacement
size for traffic. The contract documents concurrency and requested-byte limits.

The dependency fixtures pass before Cordial's meter migration. Cordial now uses
Hotpath's global allocator directly in every build; `cap` and the System-only
branch are removed. Operational budget/OS sampling, pressure, eviction, and
persistence paths are preserved. The existing allocator policy test no longer
requires an obsolete feature-gated declaration. The public live-meter fixtures
now use small allocations rather than a million objects or a 64 MiB buffer.
Their two scenarios run in one test to avoid interference through the global
meter under the default test runner.

Validation:

- `cargo test --all-features --test hotpath_heap --test hotpath_alloc_contract
  --test live_bytes --test hotpath_allocator --test salsa_eviction --
  --test-threads=1` passed the focused suite, including all seven isolated
  allocation-contract scenarios and all four cache-behavior fixtures.
- `cargo test --no-default-features --test hotpath_heap` passed all six isolated
  allocator fixtures. The final public-meter test also passed with
  `--no-default-features` and `--all-features` after combining its scenarios.
- `cargo clippy --all-features --all-targets -- -D warnings` and
  `cargo fmt --all --check` passed on the final source. Standalone compiler and
  Clippy checks covered the new allocator's pass-through and live-only paths.
- Applying `vendor/hotpath.patch` to the published source reproduces the
  vendored source byte for byte. The staged whitespace check passes while
  preserving upstream fixture bytes and unified-diff context lines.
- No Cordial command, benchmark, or full-program measurement was run.

Retained origins, exact report fields, and coverage APIs remain pending in phase
2. Process lifetime capture and report ingestion remain phases 3–5. The new meter
alone does not complete RAM attribution.

### Phase 2 exact-report checkpoint

Hotpath now emits a versioned `allocation` JSON section alongside its display
fields. Exact byte/count rows preserve function ids and accounting mode and are
independent of display metric and row limits. Timing-only reports explicitly
mark allocation recording disabled. Missing allocation values remain null.
Lifetime coverage counters expose recorder/unscoped traffic, pre-thread capture,
hidden threads, thread overflow, unavailable TLS, depth hits, cross-thread guards,
inactive recording, and dropped measurements. These counters overlap and must
not be added as a disjoint partition. Retained origins remain the next phase-2
capability; no complete RAM-attribution claim is made.

Controlled tests cover exact capture in both metrics/modes, coverage counters,
large integer serialization (without allocating large buffers), missing values,
and timing-only disabled reports. No Cordial command or full program was run.

Gate evidence: `cargo clippy --all-features --all-targets -- -D warnings`,
`cargo fmt --all --check`, the all-features JSON/traffic fixtures, and the
timing-only JSON fixtures passed. Patch reconstruction matches byte for byte.

### Phase 2 retained-origin completion

Hotpath's optional `hotpath-retained` now tags blocks with origin ids and keeps
bounded origin counters within the same allocator. Scope/thread exit,
cross-thread frees and moved/in-place realloc preserve original attribution.
Unscoped, recorder, unavailable-TLS and origin-overflow buckets remain explicit;
their outstanding totals reconcile with process requested bytes. Failed requests
leave the old state intact. Detailed mode reports registry and live prefix bytes
separately; live-only builds retain the cheap two-atomic budget meter.

The single Cordial guard construction point supplies Hotpath's snapshot provider.
Exact reports carry a pre-initialization baseline and a final heap snapshot,
while preserving lifetime peak semantics and explicit disabled/unavailable
states. Async poll unwinding restores its frame and exposes incomplete traffic.
The contract above documents the API, limits, observer effects and controlled
unit evidence. This completes phase 2's dependency capability work; it does not
complete the process/operation coverage or report ingestion in phases 3–5.

Validation:

- The final all-features focused suite passed 25 tests across `hotpath_heap`,
  `hotpath_alloc_contract`, `hotpath_json`, `hotpath_retained`, `live_bytes`,
  `hotpath_allocator` and `salsa_eviction`. The contract fixture launches eight
  isolated scenarios in the same unit-test executable; retained-origin evidence
  includes eight local-allocator cases.
- Live-only `hotpath_heap` and `live_bytes` passed all seven tests with
  `--no-default-features`. Timing-only JSON fixtures passed with
  `--no-default-features --features hotpath`.
- `cargo clippy --all-features --all-targets -- -D warnings` passed. Selecting
  both `cordial` and `hotpath` also checked the dependency's library with Clippy,
  rather than relying on Cargo's dependency lint suppression.
- Formatting, whitespace and byte-for-byte patch reconstruction passed.
- No Cordial command, benchmark or full-program measurement was run.

### Phase 3 process ownership checkpoint

`ProcessProfile` owns CLI capture before tracing/argument parsing. Configuration
relocates its durable manifest without restarting Hotpath. Command setup, result
flushes, footprint logging and normal command teardown occur before finalization.
The isolation driver returns its status through `act_with_exit_code`, so its
ordinary stack also tears down before the process owner finishes. Direct library
users of `Cli::act` can install the same explicit process owner.

Each run has a unique directory under `perf/raw/profiles/`. Each process writes
its own manifest and raw Hotpath report; the parent registers expected child
identities before spawn and records pid/exit/spawn errors. A child inherits the
run identity, parent and restart number. Actual producer features and executable,
Hotpath version, accounting/display mode and optional state/repeat/test identity
are recorded. Missing child manifests and truncated/deleted reports remain
visible to `profiling::read_run`. The manifest pieces form the run record without
multiple processes rewriting one shared JSON file.

The collector stages Hotpath's report at a unique temporary path, then validates
and publishes it under the store. Configured output paths produce an additional
export suffixed by process identity, never an overwrite of the configured base
file. Hotpath's pinned output API prevents output environment overrides from
redirecting or colliding with the reserved raw artifact. Normal/error returns,
unwind, memory-watchdog, no-room and resume exits have distinct completion
reasons; only ordinary returns promise normal workload stack teardown. A valid
report after watchdog/resume termination is still a partial workload lifetime.
Repeated finish calls preserve the first result. Uncatchable termination leaves
`running` with no completion time/reason, rather than a complete or zero report.

Inspection of the pinned dependency found no nonterminal checkpoint/snapshot
report API. Guard shutdown drains and terminates the workers. Restarting guards
does not create a valid continuous capture because transport/registry state is
process-global. Phase 3 therefore preserves manifest/staging evidence on an
interruption and does not fabricate checkpoint reports. Allocator snapshots
remain Hotpath's own API; they are not a substitute report stream.

Controlled evidence is in `tests/hotpath_lifecycle.rs`; the next checkpoint
connects test lifetime ownership and expected-report collection to the existing
perf path. No Cordial command or full-program measurement is used.

Checkpoint validation: all five lifecycle unit tests passed, including setup and
teardown, errors, unwind, controlled termination, missing/crashed children,
resumes, exports and output overrides. The all-features/all-targets Clippy gate,
formatting, whitespace checks and byte-for-byte vendor patch reconstruction
passed.

### Phase 3 test and workflow linkage

`profile_test()` is the explicit test-thread owner: declare it before fixtures
to capture setup and normal teardown. A helper thread owns the same `Capture`
used by the CLI, keeping report shutdown out of test-thread TLS destruction.
The compatibility `init_tracing` start remains available and records
`first_init_tracing`, which is explicitly a late boundary. Nextest's existing
event stream durably registers started/completed tests, including ones that
never initialize capture. The shared perf reader joins these expectations to
unique process manifests without losing original binary/test names or repeats.

Workflow child tickets carry cold/warm/incremental state and repeat through
isolation restarts. Expected child metadata survives outside the disposable
cold store. The workflow does not infer child features from its driver: each
manifest records its actual producer features. Library workflow calls without
an installed process owner still need standalone expected-child collection in
phase 5; they must not be presented as verified complete captures.

Publication is serialized with competing controlled-exit callers. Raw JSON
validation streams and ignores payloads rather than building another report
DOM while memory is under pressure. A parent-observed exited child whose
manifest remains running is collected as incomplete.

All 17 focused tests passed across `hotpath_lifecycle`, `perf_profile`,
`perf_workflow` and `perf_workflow_partial`. Workflow tests use tiny shell
stand-ins; lifecycle tests launch only their own unit-test executable. No
Cordial binary or full program was executed.

Phase 3 completion gate: all-features/all-targets Clippy with warnings denied
passed. All seven lifecycle fixtures also passed with no default features and
with timing-only Hotpath enabled; disabled allocation capture stays explicit.

### Phase 4 operation and concurrency coverage

The shared native collector now attaches Hotpath syntax facts to its existing
function inventory: direct measurement, conditional attributes, async bodies,
closures and async blocks. `operation_coverage` reads these through `IrView`.
The old debug/info-only text scan is replaced by a controlled loader fixture
containing untraced functions and escaping bodies. The allocator test verifies
the linked live meter with no guard, rather than matching declarations or Cargo
feature strings.

The shared worker runtime, round dispatch, poll-aware async task and synchronous
worker item have Hotpath scopes. Named worker scopes surround both parallel
Salsa verification paths. No synchronous guard spans an await. Hotpath supplies
thread/operation identity but has no supported cross-spawn parent-link API in
the pinned integration; the report does not invent parent allocation ownership.

Every manifest carries all ten inventory regions, controlled evidence and
remaining unverified boundaries. It also preserves collection/display overrides
that can reduce coverage. Registry overflow, depth limits, unscoped work,
recorder and TLS exclusions remain Hotpath's exact counters, to be consumed in
phase 5. Pipeline target coverage, delayed background/TLS destruction, opaque
dependency internals and external child/native heaps remain explicitly
unverified. Those are visible boundaries, not claims that a full program has
been measured. No full-program execution was performed.

Phase 4 validation: all-features/all-targets Clippy with warnings denied passed.
Controlled allocation, linked allocator, shared IR coverage, lifecycle, worker
origin and existing worker-pool behavior fixtures passed. A 4,097-byte allocation
on each of three workers survives as exactly 12,291 bytes under the named worker
operation. No Cordial invocation or full-program benchmark was used.

### Phase 5 common ingestion, storage and presentation

`MemoryCapture` embeds Hotpath's exact allocation DTO rather than recreating
accounting. The shared lifecycle reader imports workflow and test producers,
including library workflows without a driver owner. `profiled_from` joins timing
and memory from the same inventory. `ProfileUsage` prefers exact integers and
rejects count-mode formatted strings as byte data. Exact producers are not
merged by display name; `RunRecord::memory` retains process/thread/origin ids,
features, modes, raw paths, time boundaries and completion reasons. Existing
records remain readable with an explicitly unavailable capture inventory.

Standalone workflow tickets use the existing child schema and capture path.
Their metadata directory always differs from the disposable project slug,
including a project named `perf`. Failed attempts retain memory evidence even
without timing samples. Resume graph import follows both manifest parent links
and durable child expectations, preserving missing descendants with no manifest.
Parent-observed pid, exit and spawn/wait errors survive import. Malformed and
truncated manifests/inventories remain visible rather than aborting all import.

Available zero, disabled, unsupported, missing, malformed, truncated and
incomplete are distinct. Exact schema/mode validation forbids rounded fallback.
Overflow, missing data, late setup, incomplete teardown, unavailable baselines,
display overrides and all ten coverage regions remain visible. Reports separate
requested heap, allocation traffic, retained origins, lifetime peaks and the
unsupported OS envelope. Comparison preserves repeat/state/restart boundaries,
requires compatible producers and accounting, and withholds incompatible or
lossy traffic deltas. Its memory-completeness result is separate from the timing
verdict. No process/origin peaks or ending live values are added together.

The implementation guide is [memory profiling](../memory-profiling.md).
Controlled fixtures prove a retained 12,345-byte allocation through capture,
import, record storage and display, preserve integers above 2^53, retain equal
thread names with different ids, and exercise zero/disabled/count/invalid/lossy
reports and incompatible comparisons. Workflow fixtures launch only shell
stand-ins or the unit-test executable, including expected missing resumes.

Phase 5 validation:

- All-features focused suite: 47 tests passed across `hotpath_lifecycle`,
  `perf_compare`, `perf_memory`, `perf_profile`, `perf_record`, `perf_usage`,
  `perf_workflow` and `perf_workflow_partial`.
- After the final missing-resume and workflow-directory regression fixes,
  `perf_memory` and `perf_workflow_partial` passed all nine tests again.
- Timing-only (`--no-default-features --features hotpath`): all thirteen
  `hotpath_lifecycle` and `perf_memory` tests passed.
- Recording disabled (`--no-default-features`): all thirteen lifecycle/import
  tests passed, including explicit disabled capture behavior.
- `cargo clippy --all-features --all-targets -- -D warnings` passed after the
  final code changes. Formatting and whitespace checks passed.

Phase checkpoints: phase 3 commits `ac9946fa` and `4b7cd789`; phase 4 commit
`f88cf4b4`; phase 5 is recorded by the commit containing this evidence.
No Cordial command or full-program benchmark was executed. Full-program
measurement remains deferred for maintainer review and a separate discussion
of `elicit_doc`.
