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
- **Whole-crate passes are queries over facts.** The call graph and
  `source-calls` read function facts through queries, so an edit that leaves
  a function's facts equal leaves every pass that read them untouched.
  `source-calls` is now part of the per-file items query. `error-ir-scan` is gone
  as a hook, and apply's evidence is queries (`loader/database/apply_queries.rs`)
  over the per-file `error_apply_file` results: `apply_source` joins them,
  `apply_index` builds the type index, `apply_units` the conversion units and
  `apply_graphs` the graph plans for a field strategy. Apply asks for them
  through `ParsedSourceSession::apply_evidence`; no analysis run computes them,
  and nothing is written into the IR. `tests/apply_evidence_queries.rs` pins that
  each phase runs once, a second ask runs nothing, and an edit re-runs only the
  edited file's facts. Known limit: a file's facts carry its text digest, so any
  edit changes `apply_source` and the phases above it re-run; they stop being
  per-edit work only when those phases are split per file.
- **Antipatterns are per-file queries with a joined fact.** A file's walk reads
  crate-wide facts (local traits, error types, constants), so each file's
  contribution is a query, the join is a query over them, and the walk is keyed
  by the file and the joined facts (`antipattern_queries.rs`). An edit whose
  file facts come out equal never re-runs the join, and only the edited file is
  walked; one that adds an error type re-walks every file, as it must. Contract
  clauses are a per-file query too. `tests/antipattern_queries.rs` pins both.
- **Panics follow the same shape.** Each file contributes its part of the Kani
  proof graph, a query joins them, the set of functions the proofs reach is
  derived from the join, and each file's scan is keyed by the file and that set
  (`panic_queries.rs`; the key types shared with antipatterns are in
  `crate_files.rs`). An edit that leaves the file's graph equal re-scans only
  that file; a new proof that changes what is reached re-scans every file.
  `tests/panic_queries.rs` pins both.
- **Error architecture and signatures are per-file parts and a join.** Both
  collectors only ever add to their catalog or index, so each file's part is a
  query keyed by the file and the crate's value is a query that joins the parts
  in file order (`architecture_queries.rs`, `signature_queries.rs`). A body edit
  recomputes the edited file's part and no other. An edit that adds an item
  changes the crate's name resolution and rightly recomputes more widely.
  `tests/error_evidence_queries.rs` pins both.
- **Path inclusions are a per-file query.** A file's `#[path]` targets are a
  function of its syntax and its package, so each is a query keyed by the file
  and the package (`path_inclusion_queries.rs`); the workspace graph that joins
  them is cheap and stays where it is asked for. `tests/path_inclusion_queries.rs`
  pins that a body edit recomputes only the edited file's targets.
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

## Settled design: layers and milestones (C4/C5)

Written 2026-11-13, after the first profiles. Nothing below is a cache we invent;
every step is a Salsa query, an input, or an assembly of their results.

### Layers

```text
A inputs        file texts, crate graph and cfg (rust-analyzer's); the cordial policy
B file facts    one tracked query per collector, keyed by file
C item facts    per-function and per-item queries, keyed by item identity
D crate facts   definitions, call graph, path inclusions: queries over B and C
E assembly      the IR graph, built from B-D results (cheap, deterministic)
F hooks         enrichers, probes, assessors read the IR as they do today
```

- **A.** File text and the crate graph are rust-analyzer's inputs; `change_file`
  already feeds edits in. The cordial policy (`cordial.toml`) becomes one Salsa
  input, set only when its value differs, so a policy change invalidates what read
  it and nothing else.
- **B.** Each file-local collector (scope, attributes, visibility, observability,
  derives, glob imports, inline tests, error sites and chains, the error-apply
  families, panics, antipatterns) is its own `#[salsa::tracked]` query keyed by a
  `FileKey` (file id and path). Separate queries, not one `file_facts`, so an edit
  that leaves one collector's output equal leaves its consumers untouched (early
  cutoff). Outputs are `Clone + Eq` typed facts; the projection stops computing
  them inline.
- **C.** `function_evidence` and `direct_facts` exist. Add per-function syntax
  facts (`function_source_syntax`, call syntax) and per-item facts, keyed by item
  identity (module path and name, or the impl's ordinal), never by position.
- **D.** Whole-crate passes read C through queries: `definitions` (identity to
  item), `call_graph`, `path_inclusions`. A pass reads only the facts it needs, so
  an edit recomputes the passes whose inputs changed.
- **E.** The IR stays the consumer contract (probes, assessors, reporters, apply
  keep `IrView`). It is assembled each run from B-D results by moving memoised
  values into nodes: O(project) but a copy, not a recomputation. Node attributes
  hold shared values, so assembly does not serialise or deep-clone every fact.
- **F.** The hook API is public (plugins implement it), so it does not change.
  Built-in enrichers become *providers*: a per-function or per-file query that
  returns the attributes it would have written, applied during assembly. A legacy
  enricher (a plugin's) still runs over the assembled IR. Probes and assessors
  stay IR readers.

### Granularity: files, not positions

Facts keep the offsets and ranges they have, and the transformations depend on
exact edit ranges. So a comment line recomputes the facts of its own file and no
other, and the cost of an edit is the edited file, not the project. Position-free
facts would also spare that file, and need every payload reworked; they are a later
refinement, taken only if the edited file's cost is measured to matter.

### Cfg contexts (C5)

rust-analyzer reads cfg from the crate graph input, so a query argument cannot
carry it, and toggling every local crate's cfg and restoring it made every memo
stale on each run. Adding a copy of each crate under the context's cfg is not
enough on its own: rust-analyzer resolves a file in the first crate of its source
root, so a file in two crates never resolves in the second (`file_to_def` takes the
first module). So **a context is a copy of the workspace**: each local source file
copied to a virtual path under a source root of its own, and a copy of each local
crate under the context's cfg, rooted at those copies, with dependencies between
local crates following to the copies. Every query is then keyed by files and crates
the ordinary build does not use. The dependency crates are shared. The context and
leaf passes read the copies' trees with the code they always had; edits (from
`change_file` and `sync_from_disk`) are applied to each copy. `add_context_crates`
is idempotent, so in a session a context is built once. Built 2026-11-13
(`context_crates.rs`); the spike's conditions held: the copy sees its gated code,
the ordinary build is unchanged, and adding a context re-ran no query for it.

### Milestones, in order, each tested before the next

1. **Per-file collectors (B).** The harness (`FileKey`, one source carrier shared
   with the projection) and the first queries; then every other file-local
   collector. Tested by: the assembled IR equals the IR the inline collectors made
   (existing golden tests), and after an edit to one file only that file's queries
   execute (counted through the query events).
2. **Per-function and per-item facts (C), then crate passes (D).**
3. **Assembly with shared attribute values, and providers for the built-in
   enrichers (E, F).** `source-calls`, `internal_error_chain` and the other
   per-run enrichers, one at a time, each against its golden findings.
4. **Contexts as coexisting crates (C5):** the spike, then the leaf pass and the
   context pass.
5. **Counters (C2)** generalised from the priming events to every query: executed
   against reused, per stage, in the run's events.
6. **Acceptance:** the edit-loop tests below, then (and only then) measurement.

### Tests that establish each step

- **Equivalence.** The IR and the findings after a step equal those before it, on
  every fixture the suite already has.
- **Locality.** On a fixture, after editing one file: queries for other files do
  not execute; a body edit does not recompute callers whose facts are equal; a
  signature edit recomputes callers and no more. These use the query events, never
  timing.
- **Determinism.** Parallel priming interns in an arbitrary order, so equality is
  checked on content, not on database-local ids.

### Not doing

A query wrapping the whole pipeline; a second database of copied facts; a cache
of decoded attributes beside the IR; a digest or key built by hand. Where a Salsa
mechanism does the job, it is the one used.

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

- [x] C3. Persistent session process (first). `open` starts a session when none is running (set `CORDIAL_NO_SESSION` to
  opt out); it ends with the terminal that started it (the server polls for its
  process-session leader), and `cordial session start|stop|status` is the
  explicit control, `start` meaning a fresh session. A server per project
  (a short socket under `$XDG_RUNTIME_DIR`, named from the store). Clients forward `open`,
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
- [x] C4b. The call graph's 195-202 s was `active_contexts` decoding the whole semantic
  snapshot and context plan once per function (functions x project size), not work worth
  caching. Decoding the plan once per crate: 202 s -> 0.14 s on cordial (cold, no cache).
  The run-cache copy of the graph (C4a) now saves 0.14 s and is a candidate to remove.
- [x] C4c. Cold `source_ir` on cordial, 329-337 s -> 207.8 s (whole cold stage wall 213 s):
  per-function evidence is evaluated on worker threads over clones of the database before
  the projection reads it (`loader/database/prime.rs`), once for the workspace and once
  inside each leaf cfg context, because a changed cfg input makes every result stale.
  Evidence decodes from the stored value, not a clone of it. Each priming pass records how
  many functions it evaluated (`function evidence (workspace): 14056 evaluated for 14056
  function(s)`; later leaf contexts: 1255, 1, 101, 142, 76), which is the reuse counter
  C2 asked for, for this one query.
- [x] C4a. The call graph is reused from the run cache when the tree and the tracing
  policy are unchanged (`call graph: reused from the run cache`). Measured on cordial:
  call graph 194.7 s -> 0.004 s, wall 546.7 s -> 14.8 s on an unchanged tree. Any edit
  still rebuilds it, because the entry is keyed on the whole tree.
- [ ] C4. Facts as tracked queries: milestones 1-3 of the settled design above.
  Milestone 1 (per-file collectors) so far, each a tracked query keyed by file with the
  direct path as fallback and an edit-locality assertion in `tests/semantic_backend.rs`:
  `file_facts` (module scope and attributes), `error_sites_file`, `error_chains_file`,
  `error_types_file`, `error_compliance_file`, `observability_file`, `verus_file`,
  `error_apply_file`, `visibility_file`, `policy_file` (the allows/cfg/glob/inline-test/
  modularity/pageantry/derive bundle). And `file_items`: the IR assembly itself. A file's
  items (node weights, qualified paths, function syntax facts, evidence as JSON, macro
  islands) are a query, and `populate` only inserts them in the order they were built, so
  node ids are unchanged. An edit rebuilds the edited file's items and those of files whose
  functions' evidence changed (callers), and no other file's. Remaining there, by recorded cost on cordial:
  antipatterns 2.9 s, panics 2.4 s, architecture 1.1 s, signatures 0.7 s, then the
  per-function syntax facts in `populate` and the tracing inventories (4.7 s). CLI layout
  (0.8 s) is left alone: its per-file value holds syntax nodes, which cannot be a query value.
- [ ] C5. Whole-crate passes as queries (milestone 2). Cfg contexts (milestone 4) are built: the context and leaf passes read per-context copies of the workspace (`add_context_crates`), nothing is toggled, and the ordinary build's memos survive. Still to do there: prime a context's function evidence in parallel (it is read one function at a time).
- [ ] C1. Cache key hygiene: analysis-source digest in place of the binary fingerprint.
- [ ] C6. Re-baseline and record the acceptance numbers.
