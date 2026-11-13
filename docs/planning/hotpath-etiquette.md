# `hotpath` etiquette

Status: **Active.** The etiquette itself is not built. Its scanners exist on
cordial as scan tests (see "Dogfood"), and the run record reads every section they
feed. The remaining work is to extract the scanners into an etiquette and wire it
in.

A quality etiquette that checks a Rust codebase for **complete performance
instrumentation with [`hotpath`](https://docs.rs/hotpath)**: every operation
timed, every allocation attributable, every lock, channel and thread visible,
and every persisted artifact reporting its size. The point is coverage by
construction: instrumentation is not added when someone has a question, it is
there, and the gate fails when new code does not have it.

Companion to the [tracing etiquette](tracing-etiquette.md), whose function
inventory it consumes, and to [performance metrics](performance-metrics.md),
whose profiles it makes complete. The etiquette is static and checks coverage;
`cordial perf` is dynamic and reads the numbers. They stay separate.

## Problem

Instrumentation drifts. On cordial, a profile answered only the question that
had been anticipated:

- 154 of 5,085 instrumented functions carried `#[hotpath::measure]`, so a
  profile showed the stages someone had already looked at, and a stage was
  measured only after it had been slow. Fixing that took one scripted pass
  (2,157 insertions) and a scan test, `tests/hotpath_coverage.rs`.
- The profile had no storage numbers: no allocation, no size of anything
  persisted or kept, nothing about locks or channels, and no thread data a
  reader could trust (threads were unnamed).
- Mistakes were easy to make and silent. A module named `hotpath` in the
  `config` module shadowed the crate inside the macro's expansion. A measure on
  a `Drop::drop` crashes the recorder during thread-local teardown. A tracing
  subscriber set for one thread does not reach worker threads.

Each of these is a mechanical fact about the source. A static check finds them
all, and an apply can fix the common one.

## What counts

| Area | Required in the codebase | Where hotpath reads it |
| --- | --- | --- |
| Timing and call counts | `#[hotpath::measure]` on every operation (a function instrumented at `debug` or `info`) | `functions_timing` |
| Heap allocation | One `#[global_allocator]` of `hotpath::CountingAllocator`, under a feature | `functions_alloc`, per-thread bytes |
| Threads | Every spawned thread is a named `thread::Builder` | `threads` |
| Locks | `Mutex` and `RwLock` come from one module that re-exports hotpath's | `mutexes`, `rw_locks` |
| Channels | `mpsc` comes from the same module | `channels` |
| I/O bytes | The readers and writers of persisted artifacts are wrapped | `io` |
| Storage values | The store is walked once from its root, and the in-memory stores (a context's copy, the Salsa database) report themselves, all through one module | values (`val!`), in the report's `debug` section |
| Query reuse | Every `#[salsa::tracked]` function is measured and delegates to a measured inner function | the outer function counts requests, the inner counts executions |
| The guard | One place builds the guard, with every section enabled | all sections |

Futures, streams, SQL, HTTP, the server layer and the Tokio runtime are
supported by hotpath and out of scope until a codebase uses them; the etiquette
checks a section only when the codebase has the primitive that feeds it.

## Facts it consumes

The etiquette does not re-derive what other etiquettes know, and it does not
match names against names.

- **From tracing:** the function inventory (which functions are instrumented,
  at what level, with which attributes) and the function's role. A function the
  tracing etiquette would exclude from instrumentation is not eligible here.
- **New, resolved through the semantic database:** the items a path names. A use
  of `std::sync::Mutex`, `std::sync::mpsc`, `std::thread::spawn`,
  `std::process::Command`, or a function annotated `#[salsa::tracked]` is found
  by resolving the path to the definition, so an alias or a re-export is seen
  and a local item that happens to share a name is not.
- **From the manifest:** the `hotpath` dependency, its features, and the
  crate's forwarding features (`hotpath`, `hotpath-alloc`), which
  `dependency_freshness` already surveys.
- **Per-file, as Salsa queries:** the new scans follow the shape of the policy
  scans (one tracked query per file, merged by path order), so they cost
  nothing on a file that did not change.

## Rules

Rule ids are stable and each resolves through `cordial explain`.

| Id | Flags | Apply |
| --- | --- | --- |
| `HOTPATH-MISSING-MEASURE` | A function instrumented at a configured level (default `debug`, `info`) has no `#[hotpath::measure]` | Insert `#[hotpath::measure]` after the `#[instrument]` attribute. This is what the scripted pass did |
| `HOTPATH-MEASURED-DROP` | `#[hotpath::measure]` on a `Drop::drop` | Remove it |
| `HOTPATH-TRACKED-UNMEASURED` | A `#[salsa::tracked]` function without a measure, or whose body does not delegate to a measured function | Advisory |
| `HOTPATH-RAW-MUTEX` | `std::sync::Mutex` or `RwLock` used outside the configured sync module | Rewrite the path to the module |
| `HOTPATH-RAW-CHANNEL` | `std::sync::mpsc` used outside the sync module | Rewrite the path |
| `HOTPATH-THREAD-UNNAMED` | `thread::spawn`, or a `thread::Builder` with no `.name(..)` | Rewrite to a named builder, taking the name from the enclosing function |
| `HOTPATH-ALLOCATOR` | No counting allocator is declared, or more than one | Advisory (it changes the whole program) |
| `HOTPATH-STORAGE-UNRECORDED` | A function that writes or reads a persisted artifact does not report its size through the storage module | Advisory |
| `HOTPATH-LITERAL-LABEL` | `mutex!` or `channel!` given a string-literal `label` in a crate that forbids unsafe code. With the feature on, hotpath's uniqueness check for a literal label expands to an `export_name` attribute, which `forbid(unsafe_code)` rejects, so the build breaks only in the instrumented configuration | Rewrite the label to a block expression (`label = { "name" }`), which hotpath accepts without the check |
| `HOTPATH-STATIC-LOCK` | A `static` `Mutex` or channel built with the plain constructor, which the instrumented wrapper cannot replace because its constructor is not `const` | Rewrite to `LazyLock<Mutex<..>>` built with the macro |
| `HOTPATH-SHADOW` | A module, import or item named `hotpath` that shadows the crate inside a macro's expansion | Advisory, with the rename |
| `HOTPATH-GUARD` | A guard is built outside the central function, or without every section | Advisory |
| `HOTPATH-FEATURE` | The manifest does not forward the `hotpath` and `hotpath-alloc` features to the dependency | Advisory |

"Advisory" means the finding is real and cordial will not edit for it: the fix
changes behaviour or needs a human choice (a thread's name, an allocator).

## Config

`[hotpath]` in `cordial.toml`, with the usual `enabled` switch.

```toml
[hotpath]
enabled = true
measure_levels = ["debug", "info"]   # functions instrumented at these levels need a measure
exclude_functions = ["drop"]         # never measured: the recorder cannot run during TLS teardown
exclude_paths = []                   # path prefixes skipped, as in other etiquettes
sync_module = "crate::sync"          # where Mutex, RwLock and mpsc must come from
storage_module = "crate::storage"    # where artifact sizes are reported
guard_function = "crate::profiling::guard"
require_allocator = true
thread_names = true
tracked_queries = true
```

Each check can be turned off on its own by leaving its key empty or false, so a
codebase adopts the rules in the order it can.

## Output

`hotpath.csv`, `hotpath.checklist.md` and `hotpath-summary.md`, grouped by
crate then rule. The summary carries coverage as numbers, not only findings:
functions measured against eligible, primitives routed against found, artifacts
reporting against persisted. Those counts are deterministic, so they can gate in
the pre-merge comparison the performance plan already describes.

Tier: `quick`. It is a static scan over the per-file queries and builds nothing.

## Reading profiles back

The static check says the instrumentation exists; `cordial perf` reads what it
recorded. The run record carries, besides function timing: bytes allocated per
function, each thread's processor use and heap traffic, each named lock's wait,
each channel's traffic, and the values the program reported. Three points matter
to the etiquette's design:

- **Counts gate, sizes trend.** hotpath prints byte figures with one decimal of
  their unit (a figure just above a kilobyte steps by about a tenth of itself), so
  bytes allocated and bytes stored are listed when they move and do not fail a
  comparison. The values that count things (`.files`, `.crates`, `.memos`,
  `.count`) are exact numbers and become counters, which the comparison does gate
  on. A value's key says which it is, so the storage module names values by that
  convention, and the etiquette checks it.
- **Sections are data-driven.** A section appears in the report when something fed
  it, so a missing `mutexes` section means no instrumented lock was taken, not a
  failure. The static checks, not the profile, decide whether a lock is
  instrumented.
- **Unnamed things are unreadable.** A thread with no name and a lock with no
  label cannot be told apart in the report, which is why both are rules.

## What it does not do

- It does not read profiles. Whether a measured function is slow, or a stored
  artifact is large, belongs to `cordial perf`.
- It does not choose what to measure beyond the configured levels. The levels
  are tracing's own convention: `trace` marks small helpers, `debug` and `info`
  mark operations.
- It does not change an allocator, a thread's behaviour, or a lock's type
  without the author reading the finding.

## Dogfood on cordial, before the etiquette exists

The same checks run on cordial first, as scan tests in `tests/`. Each is the
scanner the etiquette will later own. Built, 2026-11-13:

1. `hotpath_coverage`: every operation is measured.
2. `hotpath_allocator`: one counting allocator, in `profiling.rs`, behind the
   `hotpath-alloc` feature; the manifest forwards the feature; one function,
   `profiling::guard`, builds every profile guard with every section unlimited.
3. `hotpath_sync`: every `Mutex` and channel comes from `crate::sync` and has a
   distinct label; raw `std::sync::Mutex` and `mpsc` fail the scan.
4. `hotpath_threads`: every thread is a named `thread::Builder`, `cordial-...`.
5. `storage_usage`: the store is counted by its top-level entries from one walk
   of its root; a context's copy and the Salsa database report their own sizes.
6. `hotpath_queries`: every tracked query is measured and calls a measured
   function, so its reuse is the difference between the two call counts.

Findings that came out of building it, each now a rule above or in the module
docs: the literal-label form of `mutex!` and `channel!` emits an `export_name`
attribute, which `forbid(unsafe_code)` rejects, so labels are passed as block
expressions and uniqueness is a scan; the instrumented `Mutex` fixes its value
type, so a trait-object coercion has to be written out; a `static` lock cannot
use the macro (it is not `const`), so it becomes a `LazyLock`.

The report reader and run record carry every section (built 2026-11-13,
`perf/usage.rs`): allocation, threads, locks, channels and values read into typed
fields, merged across tests, with the exact counts as gated counters and the bytes as
trends in the comparison. `cordial perf tests profile` prints them.

The scanners now move into the etiquette and the tests become its fixtures.

## Phases

0. **Dogfood by hand** (above). No etiquette code.
1. **Extract the scanners** into `scan.rs`, `enricher.rs`, `probe.rs`,
   `assessor.rs` and `reporter.rs`, following the standard skeleton, with the
   per-file scans as Salsa queries. Feature `hotpath_coverage`, folded into
   `quality`.
2. **Apply** for the mechanical rules (`MISSING-MEASURE`, `MEASURED-DROP`,
   `RAW-MUTEX`, `RAW-CHANNEL`, `THREAD-UNNAMED`) through the shared-IR apply.
3. **Config, explain and tiers:** the `[hotpath]` table, `explain` pages for each
   rule, tier assignment.
4. **Profile integration:** the pre-merge comparison gates on the new counts.

## Open questions

- Instruction counts (`icount`, through callgrind) are deterministic and would
  suit a gate, but need valgrind. Include as an optional section, or leave to
  the performance plan?
- Third-party codebases use `parking_lot` or Tokio. The sync module check should
  accept their instrumented equivalents; the list of accepted modules is config.
- An `async fn` is measured by a different attribute (`future_fn`). The scan
  needs the right attribute per function kind.
- Allocation is attributed to the thread that allocates, so a function that hands
  work to others (the priming workers) looks cheaper than it is; the thread section
  gives the true totals. The etiquette should say so wherever it shows allocation.
- Overhead: allocation counting and lock wrappers slow a run. The etiquette
  checks that the instrumentation exists, not that it is enabled; the features
  stay off in release builds.
