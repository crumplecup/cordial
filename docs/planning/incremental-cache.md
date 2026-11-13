# Incremental analysis: Salsa tracks the facts, and invalidation follows

**Status:** Active, 2026-11-12. Design agreed; nothing built yet. The measurements
that motivate it are in [performance-metrics.md](performance-metrics.md).

## Problem

A run on cordial itself costs about 590 s cold, and **676 s after adding one
comment line**: the incremental state is slower than cold. The cache we have is
one digest over every input (the cordial binary, every file under every package,
the toolchain, the environment); when it changes, the whole entry is thrown away
and the run is analysed from scratch. Nothing is reused across inputs, so a
change anywhere costs everything, and every command is a new process, so
nothing survives between `open`, `done` and `submit`.

Two avoidable causes, independent of the design below:

- the key includes a fingerprint of the **cordial binary**, so every edit to
  cordial while it analyses itself is a miss, including edits to code that cannot
  affect analysis (`perf/`, `cli/`);
- the key hashes **raw bytes** of every file under every package, tests and
  fixtures included, so a comment, a reformat, or an edit to a test file
  invalidates the analysis of `src/`.

## Design

Salsa already does what a correct cache has to do: it records what each result
read, invalidates only the results that read something that changed, and stops
propagation when a recomputed result equals the old one (early cutoff). We use it
for that instead of building a second invalidation scheme.

- **Facts are tracked queries.** Per-function and per-file facts (the
  collectors in the migration inventory of
  [salsa-rust-analyzer.md](salsa-rust-analyzer.md)) become `#[salsa::tracked]`
  queries over the session database, keyed by item identity, not by position.
  Today `harvest`, the per-file collectors and the enrichers are plain loops
  over files: nothing is tracked, so nothing is reused. `dialect_facts` is the
  one existing example of the shape.
- **Facts carry no positions.** A fact is the same before and after a comment
  line is inserted above it; line numbers and byte ranges are anchors attached
  from a fresh parse. That is what lets a comment edit reuse the whole tree.
- **Whole-crate passes are queries over facts.** The call graph, `error-ir-scan`
  and `source-calls` read function facts through queries, so an edit that leaves
  a function's facts equal leaves every pass that read them untouched.
- **Cfg contexts are query arguments.** The context pass today mutates the one
  database's cfg inputs for each context (`LocalCfgs::enter`), which invalidates
  what the previous context computed. A context key as a query argument lets
  contexts coexist.
- **The session survives between commands.** Salsa memoizes in memory, and each
  command is a process. The session database is kept by a persistent session
  process, so `open`, `done` and `submit` reuse it, and a file change reaches it
  as an input change. rust-analyzer's database is not persistable (its inputs
  and queries are not `persist`), so a daemon is the route that works with it as
  it is. Salsa 0.28.5's experimental `persistence` feature may later save the
  cordial-owned layer across restarts; that is untested and is not on the path.
- **What a cold start still pays.** A fresh process has an empty database and
  loads the workspace once. After that, only what an edit invalidates is
  recomputed.

## Order

The cold profile (hotpath, 2026-11-12, 722 s with profiling overhead) says where
the time is: `collect_direct` 227 s over 17,765 functions (rust-analyzer queries,
already Salsa-memoized, but only inside one process), the whole-workspace call
graph 218 s (cordial's own loop, nothing memoized), `load_reporting` 60 s, and
writing the run cache 18 s. The cfg-context passes are 25 s and are not the
problem. So the order is:

1. **The persistent session (C3).** A process that owns the session database
   across commands. After the first load, an edit is an input change; the
   rust-analyzer queries it does not touch are answered from memory, which is
   the largest share of the cold cost. This is the step that makes `open`,
   `done` and `submit` stop reloading, and it needs no query conversion to pay.
2. **Reuse counters (C2).** Executed and reused queries per run, so every later
   step has a number to move.
3. **Facts as tracked queries (C4),** for what cordial computes itself and
   re-runs in full: first the call graph, then the collectors the profile ranks.
   Output is position-free, with positions attached from a fresh parse.
4. **Whole-crate passes and cfg contexts as queries (C5).**
5. **Key hygiene (C1), reduced.** Replace the cordial-binary fingerprint with a
   digest of the analysis sources and the schema version, so edits to code that
   cannot affect analysis (`perf/`, `cli/`) stop being misses. Hashing tokens
   instead of bytes waits for position-free facts (step 3): a cached IR carries
   line numbers, so reusing it across a comment would report the wrong lines.

## Acceptance

Measured by `cordial perf workflow` against the baselines recorded 2026-11-12
(cold 591 s, warm 155 s, incremental 676 s):

- [ ] adding a `//` comment line anywhere reuses every fact: the executed-query
  counter does not rise, and the incremental state costs about as much as warm;
- [ ] editing one function body recomputes that function's file and the passes
  that read its changed facts, and nothing else;
- [ ] a signature edit recomputes its callers' facts and no more;
- [ ] the incremental state costs a fraction of cold, with the fraction recorded
  in the baseline;
- [ ] `done` after `open` reloads nothing.

## Checklist

- [x] C3. Persistent session process (first). `cordial session start|stop|status`
  runs a server per project (`{store}/session.sock`). Clients forward `open`,
  `peek`, `done`, `skip`, `submit`, `next` and `stats`; the server runs them in
  its own process with output routed back, keeps the run's loaded database, and
  on the next run brings it up to date with `SourceDatabase::sync_from_disk`
  (source edits become input changes; a changed manifest, compiled input, or the
  set of source files drops it for a reload). A command the session cannot take,
  or declines because the build environment differs, runs locally. Tested for
  agreement with a fresh load after each kind of edit. **Not yet measured** on
  cordial itself: how much a re-projection on the live database saves is the
  first number to take when measurement resumes.
- [ ] C2. Reuse counters in the run record.
- [ ] C4. Facts as tracked queries, in order of measured cost.
- [ ] C5. Whole-crate passes and cfg contexts as queries.
- [ ] C1. Cache key hygiene: analysis-source digest in place of the binary fingerprint.
- [ ] C6. Re-baseline and record the acceptance numbers.
