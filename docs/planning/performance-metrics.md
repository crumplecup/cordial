# Performance metrics: where the time goes, and whether it changed

**Status:** Active, 2026-11-12. P1 to P6 are built; P7 (first baseline and
findings) and P8 (the pre-merge step) follow the first measured run.

## Purpose

Know the runtime cost of each operation, see where a run spends its time, and
compare two points in history to find regressions and advancements. The
measurements must come from real workflows and from the real test suite, not
from manufactured benchmarks, and they must be repeatable: before a merge from
`dev` the same procedure runs again and reports a diff against the last
recorded baseline.

Two subjects, one procedure:

1. **The library on a real workflow:** cordial run on itself.
2. **The test suite:** where each test's time goes, with fixed cost
   (cargo, dependency resolution, rust-analyzer load) separated from the work
   the test exists to check. Many tests are small fixtures whose fixed cost
   dominates; that is a finding to act on, not a number to report.

## Principles

- **Real workflow first.** A cordial-on-cordial run reflects what users do:
  `open`, `submit --tier build`, `done`, and a transformation apply. A
  benchmark that exercises a path no workflow takes is not recorded.
- **Counts gate, time trends.** Wall time moves with the machine. Deterministic
  counters (Salsa queries executed and reused, cfg contexts analysed, files and
  functions projected, cargo invocations) are exact, so a change in them is a
  regression signal on any machine. Time is reported as a trend with a
  tolerance.
- **Any two records can be compared.** The point is often to see what a change
  did, and a dependency or toolchain update is such a change. A comparison
  never refuses; it lists the inputs that differ (machine, software,
  dependency versions) beside the deltas, so a slowdown can be attributed to
  `ra_ap_hir 0.0.358 -> 0.0.362` or flagged as measured on different hardware.
- **Fixed cost is its own line.** Every record separates setup from work, so
  an improvement to either is visible and neither hides the other.
- **Measurement must not distort.** Profiling overhead (the `hotpath` feature)
  runs only in profile runs. Test timing from nextest needs no instrumentation
  and is taken in a separate, unprofiled run.
- **Serialized and named.** Profile runs use `-j 1` and named target sets, never
  an unfiltered full run, so contention does not enter the numbers. A full
  profile is run only with the maintainer's agreement.
- **One record format, stored, diffable.** Every run writes a versioned JSON
  record; comparison reads two records. No result lives only in a terminal.
- **The default store, not the repository.** Records, baselines and reports live
  in cordial's default store under `perf/`, like every other cordial artifact;
  nothing is committed and no output is redirected elsewhere. A baseline is
  per machine, which the machine fingerprint already assumes.
- **A `cordial perf` command.** Collection, comparison and baselines are
  subcommands of the tool, not scripts, so they share the store, the config and
  the record types with the rest of cordial.

## What is recorded

A **run record** holds:

| Part | Content |
| --- | --- |
| Identity | git sha and dirty flag, branch, subject (`workflow:<name>` or `tests:<set>`), record schema version |
| Environment | CPU model and core count, memory, OS (the **machine**); rustc and cargo versions, `ra_ap_*` version, enabled features, run-cache `SCHEMA` (the **software**); the resolved version of every dependency from `Cargo.lock` |
| State | `cold` (empty store and run cache), `warm` (cache hit), or `incremental` (one named file edited) |
| Stages | wall time per pipeline stage: cargo metadata and resolution, database load, IR decode, ordinary pass, cfg-context passes, leaf pass, enrichers, assessors, reporting |
| Functions | `hotpath` report: calls, total, self and p95 per measured function |
| Counters | Salsa queries executed and reused, contexts, files, functions, edges, cargo invocations |
| Spread | for repeated runs: count, median, min, max, and the median absolute deviation |

For the test suite a record also holds, per test: wall time from nextest, its
binary, its stage breakdown (from the per-test `hotpath` report), the
**fixed-cost share** (setup stages over total) and its counters.

## Collection

### Library workflows

`cordial perf workflow` runs the workflows on a target (this repository by
default) with a fixed command sequence and a fixed starting state:

- **cold:** empty store and run cache, `submit --tier build`.
- **warm:** immediately again, unchanged tree (run-cache hit).
- **incremental:** a pinned one-line edit to a named file, then `submit`.
- **apply:** one named transformation through its apply path.

Each is repeated (default 3, median reported) because the first run of a
process pays page-cache and allocator costs. The deep tier is not part of the
default set.

External pinned workspaces can join the set later; a record's subject names its
target so results never mix.

### Test suite

Three layers, cheapest first:

1. **Timing (unprofiled).** `cordial perf tests timing`: nextest's
   machine-readable per-test durations (JUnit or libtest JSON) over a named test
   set, `-j 1`. Ranks tests and
   binaries, flags tests whose duration is far above their assertion work.
2. **Per-test breakdown (profiled).** With the profile feature, each test
   process writes its `hotpath` report and counters to a file named from
   `NEXTEST_BINARY_ID` and `NEXTEST_TEST_NAME`. The guard starts where every
   test already starts (`cordial::init_tracing()`), so no per-test edits are
   needed. `cordial perf tests profile` merges the files into one run record
   and computes the fixed-cost share.
3. **Fixture accounting.** From the per-test records, group by fixture shape
   (single-file crate, workspace, with or without dependencies) to see which
   shared setup, if any, would pay for itself.

## Comparison

`cordial perf compare [<baseline>] [<candidate>]` reads two records and prints:

- **Counters:** exact differences, always shown; any increase in a gated
  counter fails the comparison unless the record explains it.
- **Stages and functions:** change in median with the spread of both sides; a
  change counts only when it exceeds both the tolerance (default 10%) and the
  larger spread. Ordered by absolute time moved, so the largest effect is first.
- **Tests:** per-test and per-binary change, the tests that moved most, and the
  change in total fixed-cost share.
- **Changed inputs:** the environment differences, grouped as machine, software
  and dependency, each with its before and after. Dependency and software
  changes are context for the deltas, not a reason to withhold them. A machine
  change adds a caution to every time delta (counters stay exact) and
  `cordial perf compare` still prints them.
- **Verdict per subject:** `regressed`, `improved` or `unchanged`, each with the
  changed inputs that accompany it.

Baselines are named records in the default store under `perf/`;
`cordial perf baseline` marks one, and the latest baseline recorded from `main`
is the default comparison. Recording a baseline is an explicit step, not a side
effect of a run. `cordial perf list` shows the records and baselines kept.
Measurements are history, not a cache a later run rebuilds, so `cordial clean`
leaves `perf/` in place (it is a curated path of the store).

## Pre-merge procedure

Before promoting `dev` to `main`:

1. Run `cordial perf workflow` and `cordial perf tests profile` on the named
   test set, on a quiet machine (the harness checks load and refuses a noisy
   start).
2. Run `cordial perf compare` against the `main` baseline.
3. Resolve each `regressed` verdict (fix it, or record why it is accepted in the
   merge notes) and keep the new records.
4. After the merge, record the new baseline.

This sits beside, and does not replace, the existing gates. The all-tier gate
remains the correctness check; its cold and warm timings are the input to the
tier partition in [Gate tiers](gate-tiers.md), and the stage timings recorded
here should replace the ad hoc numbers used there.

## Checklist

- [x] **P1. Run record and storage.** Versioned JSON schema, environment
  fingerprint, default-store location under `perf/`, read and write in a small
  module with a round-trip test, and the `cordial perf` command skeleton
  (`list`). Landed: `src/perf/` (`RunRecord`, `PerfEnvironment` with a
  fingerprint, `PerfIdentity`, `Spread`, `PerfStore`), records at
  `{store}/perf/records/{id}.json`, format version 1, refused when unknown.
- [x] **P2. Test timing ranking.** `cordial perf tests timing <set>` runs a
  nextest filterset named under `[perf.tests.<name>]` in `cordial.toml` (or
  `--filter`), `--repeat N` times on `--threads` (default 1), and saves a run
  record with each test's median and spread. Nextest's raw events are kept under
  `{store}/perf/raw/` as they arrive, and `--from FILE...` rebuilds a record from
  them if a long run is interrupted. The output ranks binaries by total time and
  lists the slowest tests. The `all` set is every test.
- [x] **P3. Workflow harness.** `cordial perf workflow [name]` runs the
  whole pipeline against the project with an isolated work store under
  `{store}/perf/work`: **cold** (`open` on an empty store), **warm** (`submit
  --tier build` straight after) and, when `[perf.workflows.<name>] edit = "<file>"` is set, **incremental** (one
  comment line appended to that file, then `submit --tier build`; the file is
  restored afterwards even on failure). `open` reuses a list while the tree
  looks unchanged, so only `submit` is guaranteed to run the pipeline. A run
  that did not rewrite the ledger keeps only its process time and records
  `pipeline_ran = 0`, never the previous run's stages.
  `--repeat N` (default 3) takes each stage's median. Stage timings come from the
  gate ledger the run writes (`phase`, `pipeline`, `enrich`, `assess`, `report`
  hooks), plus the process wall time. One record per state, subject
  `workflow:<name>`. Not built: the **apply** state and the quiet-machine check;
  the harness does not look at machine load, so run it on a quiet machine.
- [x] **P4. Counters (partial).** What the pipeline exposes exactly: hooks run per stage
  (`hooks.<stage>`) and total hook invocations, written to every workflow record.
  A rise in any gated counter regresses the comparison. **Not available:** Salsa
  queries executed and reused. rust-analyzer's `RootDatabase::new` takes only an
  LRU capacity and offers no event callback, so the counts would need a wrapper
  database; that is a separate piece of work. Files, functions and cargo
  invocations are likewise not yet counted.
- [x] **P5. Per-test profile.** `cordial perf tests profile <set>` runs the set
  with `--features full,hotpath` and `CORDIAL_PERF_PROFILE_DIR` set. Each test
  process starts a `hotpath` guard from `init_tracing` (no per-test edits) on a
  helper thread, so the report is written before the process ends; the report is
  named from `NEXTEST_BINARY_ID` and `NEXTEST_TEST_NAME`. The profiles are joined
  to the timings: per-test slowest functions, **fixed cost** (the totals of
  `load_reporting` and `project_target`: loading the workspace and projecting it
  into the IR), the run's functions by total time, and the fixed share. The
  record's subject is `<set>+profile`, since profiling slows what it measures.
  `hotpath` timing reports carry totals and p95 but no self time, so a
  function's self time is not recorded.
- [x] **P6. Compare.** `cordial perf compare [first] [second]` and `cordial perf
  baseline <id>`. A timing counts as moved only beyond both the tolerance
  (`--tolerance`, default 10% of the earlier median) and the larger spread.
  Gated counters are exact: a rise regresses, a fall improves; `repeats`,
  `threads` and `profiled` describe the run and do not gate. The verdict is
  regressed if any gated counter rose or any stage or test regressed, improved if
  none did and something improved, else unchanged. The comparison always prints:
  changed inputs (machine, software, dependency) with before and after, and a
  hardware caution when the machine differs. Baselines are marked explicitly in
  `{store}/perf/baselines.json` and shown by `perf list`; `--strict` exits with an
  error on a regression.
- [ ] **P7. First baseline and findings.** Record the baseline for the current
  `dev`; write up the fixed-cost findings for the tests and the decisions they
  imply (lockfile and offline fixtures, shared loaded workspaces, a loader path
  for single-file fixtures, fewer real dependencies in fixtures).
- [ ] **P8. Pre-merge step.** Add the procedure to the merge checklist and
  `CLAUDE.md`.

## Known observations that motivate this

- Fixture tests spend most of their 5 s in `cargo metadata` and the
  rust-analyzer load. Declaring a real dependency in a fixture raised the
  Verus tests from about 5 s to about 16 s each (2026-11-12); no measurement
  existed to show that cost when it was introduced.
- IR decode (about 11.8 s) and the ordinary pass (about 70 s) dominate a cold
  cordial-on-cordial run.

## Open questions

- Which external workspaces, beyond cordial itself, are pinned for workflows.
- How a baseline is shared if more than one machine is used. Decided for now:
  records stay in each machine's default store and are not committed, so a
  comparison across machines is allowed and flags its time deltas as measured on
  different hardware.
