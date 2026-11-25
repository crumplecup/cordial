# Contexts as flips: handoff (2026-11-25)

Written so that someone with no memory of the session can carry on. Read [contexts-as-flips.md](contexts-as-flips.md) for
the design and the status sections in order; this file is the state of the work, how to measure it, what to do next,
and how Erik wants the work done. `/tmp` (including the session's scratchpad) is RAM and is gone after a restart: what
mattered is here or in the repository.

## 1. Where things stand

**Goal.** Contexts (cfg variants such as `test`, `kani`, `creusot`) are analysed as *flips* of the one base crate's cfg,
not as copies of crates (`CfgContextMode::{Copies, Flips}`; copies is the default until flips is trusted). Salsa
recomputes only what read a flipped atom. Phase 3 deletes the copy design (`context_crates.rs`, S1-S4), gated on parity.

**The principle behind the work this session.** A reader must depend on what it read and nothing wider. A change to one
name, one module, one crate attribute must not re-run what read another. Every step was a unit test that failed first
(counting the queries that execute), then the change.

**Done and committed (all on `dev`, HEAD `34aeaf09`, tree clean).**

| commit | what |
|---|---|
| `a0901c6a` `bcffa111` `c0c77474` `faa61d6c` | `salsa::keyed` (vendor/salsa/src/keyed.rs): a dependency on one key, found or not; `touch`, `touch_all(db, scope)`, `unrecorded`; persisted as a map; floors are per scope (crate) |
| `ce5fa279` | module scopes read a name at a time (`ScopeKeyLt`, `DefMap::scope_read/scope_get`); cells, macro definitions and derive helpers out of `DefMap`'s equality (`DefMapPair`); block maps hold their own modules |
| `a337b3b8` | a crate's attributes and preludes are a cell, a tracked field each (`CrateFactsLt`) |
| `243348c8` `8f49f180` `e7df5745` | a module's `origin` is its own field; `modules_for_file` reads origins and a per-file list (`crate_modules_in_file`); traits in scope are a key of their own (`scope_traits`); projections `crate_module_cells`/`crate_extras` persisted |
| `e4ad4af4` | upstream's 500 `hir_def` tests run (a second); `ra_ap_hir_def` is a workspace member |
| `2e9f8898` | every segment region says `keyed_reads` and `keyed_touches` |
| `9c5142f8` `12b66cb1` | panic event; a guard test |
| `34aeaf09` | the plan's status section |

**The copies-mode panic is fixed** (`no db attached` in `InferenceResult::for_body_`, `main` thread, span
`observability_file`). Cause: a crate's map built for the first time called `touch_all`, one global floor, so every keyed
read in every other crate was "changed"; copy crates are made in later revisions, so original crates' memos went stale
and salsa re-executed their dependencies *while verifying a memo*, outside `attach_db`. The floor is now per crate. Guard:
`making_the_map_of_one_crate_for_the_first_time_runs_nothing_of_another_crate_again` (tests/cfg_atoms.rs).

**A latent hazard remains.** Salsa deep-verification can execute hir-ty queries at depth 0 with no `attach_db` whenever
a memo that read inference is *legitimately* stale in copies mode (cordial's `extract_file` attaches only when it
executes, not when its dependencies are re-run during verification). The trigger is fixed, the class is not.

**Measured on `elicit_doc`, cold, with the release binary built at `e7df5745`** (so *not* including `8f49f180`):

| queries executed in the region | copies | flips (earlier today) | flips (now) |
|---|---|---|---|
| ordinary evidence phase | 337,206 | 418,629 | **338,336** |
| contexts | 46,872 | 84,893 | 84,121 |
| of which harvest | 35,704 | 75,618 | 74,764 |
| finish_flips | none | 57,574 | 53,754 |
| leaves | 9,686 | 1,896 | 1,877 |
| inventories | 2,779 | 61,673 | 47,217 |

Superseded memos (`eviction.cfg_context.deferred_released`): copies 1,175, flips 71,137. `eviction.body_with_source_map
.dropped`: copies 4,950, flips 10,791. Findings: 85 files in each mode, **0 differing** (the four tracing findings that
differed earlier no longer do). Wall times: copies about 29 s, flips about 57 s on a quiet machine; treat wall time as a
weak signal only.

**Stale memos in flips** (`salsa::stale` events): 35,259 before the scoped floor, **15,113 after**, and `keyed` is no
longer a cause (it was 17,496). What remains, by cause:

| memo <- what changed | count |
|---|---|
| `Body::of_` <- `Body::with_source_map_` | 7,379 |
| `resolve`, `direct_facts` (cordial) <- `crate_module_cells` | 6,684 (should fall with the per-file list `8f49f180`: unmeasured) |
| `direct_facts`, `InferenceResult` <- `data` (a module cell's whole data) | 658 |
| the rest | under 300 each |

## 2. What to do next (in this order)

1. **Measure `8f49f180`.** One release build (about 8 minutes), then one flips run and one copies run (about a minute each,
   section 4). Expect `resolve`/`direct_facts` <- `crate_module_cells` to nearly vanish. If not, find who still reads the
   whole module set (`DefMap::modules()`, `own_modules`, `declared_query_keys` are the whole-set readers).
2. **Why flips drops twice the bodies copies does, and what that costs.** A memo that is dropped and recomputed cannot
   backdate (no old value to compare), so everything above it goes stale though the value is the same: that is the
   likely source of the 7,379 stale `Body::of_`. The drops are `evict_regenerable` / `evict_quietly`
   (`src/loader/database/eviction.rs`, called from `retire_crates` after a context). In flips `retire_owned_values`
   selects nothing (roots 0), so check whether `evict_quietly` and the LRU caps should run after a flipped context at
   all. First write a unit test: a flipped pass whose bodies are read, then a context finished, then the bodies read
   again, counting `Body::with_source_map_` executions.
3. **Harvest and inventories.** 30.9k macro expansions in the harvest (copies: 5.1k) and 19.4k in inventories (copies:
   1.2k). After 1 and 2, re-read the stale causes by region (`byregion.py`); what is left is first-time work or a
   dependency that is still too wide.
4. **`finish_flips`** (53k queries): `settle_bodies` re-reads every body of every crate it reaches.
5. **The latent attach hazard** (above): consider `ra_ap_hir::attach_db` at the entry points that run top-level salsa
   queries on `main` between mutations. The database is borrowed mutably between contexts, so it has to be per call.
6. **Parity and Phase 3.** `elicit_doc` is identical now. Amenable was last compared (4 of 1,059 findings differed) before
   all of this and has not been run since; that needs Erik's go-ahead and a stated prediction.
7. Whole-map fields still read as a unit: each of `prelude`, `macro_use_prelude` and the crate attributes is one tracked
   field, not per-name. Per-name keyed reads are possible with `ScopeKeyLt` if a run ever shows a flip changing them.

## 3. How Erik wants the work done (he has said all of this more than once; the cost of ignoring it is his time)

- **Cost check before every command.** Say what it costs and why a smaller command will not do. Named tests only
  (`cargo nextest run --test X test_name`), never a test group. At most one heavy job at a time. A cordial release build
  is about 8 minutes and needs its reason stated first; settle everything the next runs need before it starts.
- **Use the tracing that exists, at a verbose filter, before rebuilding to add any.** `RUST_LOG=salsa::stale=debug,
  cordial::segment=info` on the built binary answered "why was this recomputed" with no rebuild. Tracing events, not
  prints, backtraces or debuggers (`gdb` is also not permitted in the sandbox). Foresight: decide the events and
  measures a run needs *before* the first build.
- **Unit test first, shown red, with a counterpart that must still re-run** (soundness). Deterministic counts of
  executed queries (the `Runs` layer in tests/cfg_atoms.rs counts `salsa::function::execute` by query name).
- **Gate and commit are separate steps.** Run `cargo clippy --tests -- -D warnings` alone, read its output, commit only
  if it is empty. Never `;` or `&&` a commit after the gate. No `#[allow]`. One logical change per commit.
- Wall times are a weak priority signal. Look for algorithmic waste: repeated calculation of the same thing.
- No `cargo` wrapped in `time`; use `rg`; no invasive git (add, commit, status, diff, log only); `/tmp` is RAM.
- Do not stop to ask to be micromanaged: decide, say what was decided, continue. Do not run amenable or elicit_doc
  without a stated falsifiable prediction (elicit_doc is cheap, about a minute; amenable is not).
- Durable lessons go to the memory directory (`~/.claude/projects/-home-erik-repos-cordial/memory/`, index `MEMORY.md`);
  read it first. Notes there that matter most here: cost-check-before-every-command, verbose-filter-before-rebuild,
  instrument-every-site-beforehand, gate-before-commit, no-benchmark-asks-after-commits.

## 4. How to run and read the measurements

Binary: `cargo build --release` (the one on disk is from `e7df5745`, older than HEAD). The project is
`/home/erik/repos/elicit_doc`; its store is `~/.cordial/elicit_doc/` (`metrics.json`, `findings/`, `perf/runs.csv`,
whose 11th column is wall seconds).

```
# flips: a temporary config in the project (delete it afterwards); copies: no file at all
printf 'cfg_context_mode = "flips"\n' > /home/erik/repos/elicit_doc/cordial.toml
./target/release/cordial clean -p /home/erik/repos/elicit_doc            # cold start
RUST_LOG=salsa::stale=debug,cordial::segment=info,cordial::panic=error \
  ./target/release/cordial open -p /home/erik/repos/elicit_doc > flips.log 2>&1
rm /home/erik/repos/elicit_doc/cordial.toml
cp ~/.cordial/elicit_doc/metrics.json flips-metrics.json; cp -r ~/.cordial/elicit_doc/findings flips-findings
```

- Use `cordial open`, not `cordial run` (its rendering stage fails with "homecoming std reporter requires assessor
  findings"); `open` takes no `--tier` (the default is `build`).
- Do not pass `CORDIAL_QUERY_STEPS` for timing: it turns on cordial's spans and costs what a profile does.
- `metrics.json` has `segment.<region>.{micros,queries,verified_memos,verified_edges,keyed_reads,keyed_touches}` summed per
  region, and `notes` with `segment.<region>.top` (the six queries executed most) and `.verified_top`.
- Analysis scripts (in `docs/planning/contexts-as-flips-tools/`, Python 3, no dependencies):
  `sed 's/\x1b\[[0-9;]*m//g' flips.log | python3 stale.py` (stale memos by query and by cause);
  `... | python3 byregion.py` (the same, attributed to the region that ends after each event);
  `python3 cmp.py copies=copies-metrics.json flips=flips-metrics.json` (regions side by side, eviction counters).
- Regions are paths such as `projection/contexts/context/harvest`; a metric's name replaces `/` with `_`.
- To debug a failing real run, read it at `RUST_LOG=cordial=debug` first: the span path in front of every event is the
  call chain; `salsa::function::execute` events say the query-stack depth (`depth 0` is a call from cordial code, with no
  query above it).

## 5. Tests that matter and what they cost

Cheap (use these): `cargo nextest run --test salsa_keyed --test cfg_atoms --test salsa_forget` (under a second);
`cargo nextest run -p ra_ap_hir_def` (500 upstream tests, 1.3 s; after a change to what queries execute, the
incremental ones need `UPDATE_EXPECT=1 cargo nextest run -p ra_ap_hir_def` repeated until stable, and the diff read:
each expect block is updated once per run).

Slow, because they load the sysroot in a debug build (20-60 s each; one at a time, chosen for the change, never the
file): `cfg_contexts`, `saved_contexts`, `context_flips`, `segments`, `leaf_functions`, `flip_scope`. The one worth
running after a persistence or projection change is
`cargo nextest run --test saved_contexts a_restored_run_executes_few_rust_analyzer_queries` (about 55 s): it fails if a
restored run executes queries it should be told.

## 6. Things learned the hard way

- A persisted tracked function must take **one** argument: two arguments make salsa intern them, and persisting an
  interned-arguments struct panics ("was not interned in the latest revision"). Use an interned key struct
  (`FileInCrateLt`, `ScopeKeyLt`).
- Every saved salsa ingredient must serialize as a **map** (cordial's chunked saver rejects a struct: "an ingredient is
  saved as a map, and this was a struct"). The key table is saved as `k:<bits>` / `f:<bits>` entries.
- `salsa::keyed::unrecorded` suppresses only field reads in the frame that opened it; a query read inside it is recorded
  and logged. Never run a query inside it. A reader reads the data unrecorded and then says its key with `read`.
- Events on the target `cordial::nameres` are counted by a test layer (`LookupFailures`): new events there break
  `a_module_the_map_does_not_have_says_which_query_asked_for_it`. Use `cordial::scope`.
- `ra_ap_hir_def` is a workspace member (root `Cargo.toml`) so that its dev-dependencies resolve; its manifest carries
  salsa's `persistence` feature so `-p ra_ap_hir_def` builds alone. Never add `-p cordial` to unify features: it builds
  317 test binaries. Because it is a member, `cargo clippy --tests` now lints the vendored crate's library.
- `cp` is aliased interactive in this shell (`/bin/cp -f`); the auto-mode classifier denies broad `pkill` and a gate
  chained to a commit.
- A memo that was dropped cannot backdate; a flip that changes one name touches one key (`keyed_touches` is small, 472 on
  `elicit_doc`); a first map build raises one crate's floor, never another's.
- hir_def's `DefMap` after `freeze` holds no modules of its own: `modules`/`cells` are empty for a crate map; readers go
  through `crate_module_cell(s)`, `crate_modules_in_file`, `crate_extras` and `CrateFactsLt`. Block maps keep their own
  cells and `frozen` is false for them.

## 7. Map of the code

- salsa: `vendor/salsa/src/keyed.rs` (keyed ingredient), `zalsa_local.rs` (`unrecorded`), `flatten.rs` (`stale_event`,
  deep-verification counters), `function/execute.rs` (execute event with depth).
- hir_def: `vendor/ra_ap_hir_def/src/nameres.rs` (`ScopeKeyLt`, `scope_read`, `scope_traits`, `CrateFactsLt`,
  `crate_module_cell(s)`, `crate_modules_in_file`, `touch_changed_names`, `DefMapPair`), `resolver.rs`, `visibility.rs`.
- cordial: `src/segment.rs`, `src/tracing_init.rs`, `src/run_metrics.rs`, `src/loader/database/contexts/{flip,pass,
  leaves}.rs`, `projection.rs`, `eviction.rs`, `def_map_cutoff.rs` (fingerprints; whole-map dependence).
- tests: `tests/salsa_keyed.rs`, `tests/cfg_atoms.rs` (most of the unit tests), `tests/cfg_contexts.rs`,
  `tests/segments.rs`, `tests/flip_scope.rs`, `tests/saved_contexts.rs`.
