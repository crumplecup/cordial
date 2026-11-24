# Contexts as flips of one crate

**Goal.** A cfg context costs what it changes. The workspace's crates are analysed once; a context flips the atoms
it differs by, salsa re-runs what read them, the findings are harvested, and the next context flips from there.
Memory is the base plus one delta, whatever the number of contexts. This is what lets cordial run on itself.

The copy design (S1–S4, `context_crates.rs`, `context_shares.rs`, `Crate.canonical`, the closure rules) predicts which
modules a context cannot affect from the words in the files, and takes those from the original. It is replaced, not
tuned: on amenable it still rebuilds 5,815 of 8,697 copy modules (67%) and the copies' resolve phase costs as much as
the originals' (19 s against 16 s, 454 copy maps).

## What is established (2026-11-24)

* **Contexts add and remove atoms.** `+creusot +feature="chrono" ... +kani +verus_keep_ghost +windows -unix` is one
  amenable context (`SemanticContextKey::enabled`/`disabled`, `LocalCfgs::diff`). Flips are not additive-only.
* **One flip reaches a lot.** The first amenable context flips gates in 553 files of 49 crates; later ones 3–9 files.
* **Contexts are discovered one after another** (`plan.rs`): each is "this clause, on top of the current state", and
  each harvest copies its results into the IR before the copy is retired (`finish_context`, `retire_crates`). A
  sequential flip fits this; nothing in the IR needs two contexts alive at once (to be verified, 1d).
* **Done and measured, in place** (`tests/cfg_atoms.rs`): item trees ask one atom at a time; a finished definition
  map is cells read by module; a flip of one atom runs one item tree and one body, the same in 4 and in 40 modules;
  the impl and lang-item scans run once and everything they ask is memoized.
* **Not done: the definition map of a changed crate is rebuilt whole.** `DefCollector` is one global fixpoint
  (`resolution_loop`: imports, glob propagation, macro expansion feed each other) that salsa cannot see into. It is
  the cost left, and it is per changed crate, not per workspace: a crate whose item trees do not change is not
  invalidated at all.
* Salsa here has fixpoint cycles (`cycle_initial`, used in `hir_ty` for variances, layout, inference).
* The core compiler queries never ask which crate a file belongs to (`relevant_crates` is used by the IDE's
  syntax-to-def, which already handles a file in several crates, and by the tool). Own file copies are not a
  rust-analyzer requirement.

## What is not known, and the plan answers before it builds

1. How much of a flip's definition-map work is crates that really change, and within them how much of the map.
2. How much of `resolve` is import propagation and how much is macro expansion (expansions are memoized by call id
   and would be reused in place).
3. Whether flipping back and forth costs more than the copies did (salsa keeps one memo per key, so a flip back
   re-runs the delta).
4. What stays in memory after a flip back (interned ids and memos of items that exist only in the other state).

## Phases

Each phase starts with its test, lands only green, ends with a report that leads with whether it works.

### Phase 0. Measure the flip's definition-map work (no behaviour change)

* 0a. Library event on every definition map built (`cordial::def_map`): per map, how many modules are equal to the
  same module's data in the base (by crate, with the reason a copy declared them affected), plus how many import
  iterations ran and how many macro expansions executed against were already memoized. Fixture first: a crate with
  a gate in one module of 30 asserts the counts.
* 0b. One amenable run (events confirmed captured on the fixture first). Output: for the contexts, the share of the
  copy maps' resolve time that is crates with no changed module (free in place), crates with some, and the changed
  fraction inside those.
* **Gate:** the numbers say what Phase 1 gets by itself and how large the Phase 2 prize is. Nothing else starts
  before this is read.

### Phase 1. Contexts as flips, collector unchanged

* 1a. Parity fixture (`tests/context_flips.rs`): for each fixture context, the harvest through flips equals the
  harvest through copies (same findings, same function keys). Written first; red until 1b–1c.
* 1b. Flip API on `SourceDatabase`: apply a context's diff to every local crate's `cfg_options`, harvest, and apply
  the next context's difference to the current one. Contexts ordered to minimise the atoms changed between
  neighbours (symmetric difference of their atom sets); the ordinary state is one context.
* 1c. Harvest keyed by the original `FunctionLocation` directly; `copy_functions.rs` and the copy-to-original maps
  are not needed.
* 1d. Memory after a flip back: hotpath-alloc on the fixture, then on amenable; if ids and memos of gated-only items
  stay, retire them with the existing `retire_*` keyed by what the current state no longer owns.
* 1e. Save and restore: the state is saved at the ordinary cfg (flip back first); `saved_state`,
  `saved_contexts`, `base_state`, `a_restored_run_executes_few_rust_analyzer_queries` stay green.
* 1f. Both modes behind an option until parity is shown (`copies` | `flips`); then one amenable run with `flips`:
  findings diffed against the copies run, time, peak, and the events of 0a.
* **Gate:** parity on every fixture and an identical findings set on amenable. Report time and peak against
  4.1 GB / 3 min 35 s.

### Phase 2. A changed crate's map in proportion to its delta

Chosen by Phase 0's numbers. If a changed crate is mostly unchanged, this is the work; if flips already cost little,
it is not started.

* **Option A, additive continuation.** Keep the base crate's finished collector state (glob graph, unresolved
  imports and macros, `from_glob_import`, textual macro scopes) and, for a flip that only adds items, imports or
  macro calls, seed the new facts and continue the loop. A flip that removes anything, or touches `macro_use`,
  `cfg_attr` on a macro, or legacy textual scope, rebuilds whole; every fallback is counted in an event. Cheap,
  partial, and contexts remove atoms, so its reach has to be measured in 0b first. Memory cost: the collector state
  is kept per changed crate.
* **Option B, name resolution as salsa queries.** The principled version of "use salsa for it": a module's scope is a
  tracked query that resolves that module's imports by asking the scopes they name; cycles (globs, mutual imports,
  macro-generated imports) use salsa's fixpoint cycle with a monotone join; item and import facts are read from
  item trees, so a flip re-runs the scopes whose facts or inputs changed and equal scopes stop it. It also makes the
  cells' reads exact. It is a rewrite of `collector.rs` (about 3,000 lines) and of the order-dependent rules
  (glob versus explicit precedence, `macro_use` and legacy textual scope, attribute-macro reseeding).
* **Rejected: logging each derivation step and replaying.** Invalidating a step needs the negative dependencies
  (what a lookup did not find, what a glob shadowed), which is truth maintenance and the likeliest place to be
  silently wrong.
* **Oracle for either option.** The existing collector stays as the reference. Equal maps module by module (scope,
  visibility, macros, diagnostics) on: all `nameres` tests; the sysroot crates; every crate of amenable under every
  context. A debug mode builds both and compares; the event counts disagreements, which must be zero before the old
  path is removed.

### Phase 3. Delete the copy design

Only after Phases 1 and 2 are green and the amenable run is equal: `context_crates.rs`, `context_shares.rs`,
`contexts/{flips,gates,reuse,copy_functions}.rs` (keeping what discovery in `plan.rs` needs), `Crate.canonical`,
`shared_modules`/`own_modules`/`take_shared_child`/`SharedWithOriginal`, the retirement keys for copies, and their
tests. Each removal is its own commit with the targeted tests green.

## Order and stopping

0a → 0b (read) → 1a–1f → measure → decide Phase 2 → 2 → 3. Stop and report if: a parity test cannot be made equal
(the flip model drops something copies had), a flip back costs more than the copies did, or the oracle finds a map
the incremental path cannot reproduce.

## Rules that apply

Tests first and red until the change; no `#[allow]`; no unsafe in new code; typed errors; library tracing and measures
rather than probes; one run on a large workspace with the answering events confirmed recorded first; the report leads
with whether it works.

## Progress

*2026-11-24, Phase 0 folded into Phase 1.* The audit planned for Phase 0 compared a copy's modules with the
original's; that cannot be done honestly, because a copy's ids contain the copy's crate (equality and the stable hash
both differ for identical modules). In place the ids are the same, so the audit is exact and is part of the flip
(`audit_flip`, event `cordial::def_map`: `modules`, `modules_changed`, `modules_added`, `modules_removed`,
`crates_changed`).

Done, each with its test, all green in `Copies` mode too:

* 1a/1b. `CfgContextMode::{Copies, Flips}` (option, and `cfg_context_mode` in `cordial.toml`). `Flips` writes the
  context's cfg into the local crates, harvests from the same crates and files, and writes the ordinary cfg back
  (`contexts/flip.rs`; `ContextCrates::in_place`; nothing is retired after it). `tests/cfg_contexts.rs` runs every
  test in both modes (14 tests), and `tests/context_flips.rs` asserts that the cfg is written once per context and
  no file or crate is copied.
* 1c (relevance). A flipped context keeps a function's facts when they are not the ordinary ones, an exact
  comparison, instead of when its words match a gate's names (`a_function_keeps_a_contexts_facts_only_when_...`).
* The audit event, tested on a 7-module crate with one gated module: 7 modules, 1 changed.

Not done: ordering contexts by the atoms between neighbours (each context now restores the ordinary cfg first);
memory after a flip back (1d); save and restore with flips (1e); the amenable measurement (below); Phase 2 and 3.

## Status (2026-11-24, after Phase 2)

**Built, with unit tests that fail without each part:**

* *Phase 1, flips with no copies anywhere.* `CfgContextMode::Flips` (option, and `cfg_context_mode` in
  `cordial.toml`). The cfg contexts pass and the leaf stage both write a context's cfg into the ordinary crates,
  read it from the same crates and files, harvest by an exact comparison with the ordinary facts, and take the
  nearest context next (`SemanticContextKey::atoms_apart`). A context's cfg is written straight over the one before,
  so what it computed stays for the next (`a_context_that_keeps_the_atoms_of_the_one_before_...`); the flipped state is
  forgotten once, when the pass is done: the ordinary cfg is written back, what the flips changed is forgotten
  (`forget_memos_changed_after`), the saved queries keyed by what the map no longer declares are forgotten with typed
  reasons (`hir_def::declaration`), and the maps are made again before the save. `tests/context_flips.rs`,
  `tests/cfg_contexts.rs` (every test in both modes), `tests/leaf_functions.rs` (zero copies).
* *Salsa.* `forget_memos_for_inputs`, `forget_memos_changed_after` (never a query that made tracked structs),
  `memo_keys`, `existence` (exists, deleted, replaced, from a stored generation), `extension` (state of one database).
  `tests/salsa_forget.rs`, `tests/salsa_extension.rs`.
* *Phase 2, a crate's definition map in proportion to what changed.* A crate keeps what its last map was made from
  (`nameres/previous.rs`, in the database's own extension) and takes from it every module that nothing it was made of
  has changed, with the ids it had. A file is changed when its item tree is not the one the map was made from, and the
  closure over imports, parents and macros that the copy design had runs from there, plus the cases a finished map
  cannot show: unresolved names, glob sources that were empty, macros that changed, modules a changed file no longer
  declares, and modules of other crates that were read (compared by the address of what they held, which also keeps
  the dependence on them). **Checked against a map made from scratch**, after flips in every direction, in a crate
  with every kind of module and in two crates (`tests/cfg_atoms.rs`); each rule was removed in turn and the check fails
  without it.

**Not built, and why:**

* *Phase 3, deleting the copy design* (`context_crates.rs`, `context_shares.rs`, `Crate.canonical`, the word-matched
  file lists, `shared_modules`, `Copies`). It is gated by one parity run on amenable (findings equal to a run with
  copies), as the plan says, because deleting is not undone by a test. The default is still `Copies`.
* The block maps are not made from the map before (they are small, and made again whole).
* A restored process has no previous map, so its first map of each crate is made whole.
* Not measured on amenable since Phase 2: every number in this file before this section was taken with the whole map
  made again.

**Known conservative cases** (a module is made again where it could have been taken): a module that has anything
unresolved, calls a macro when any macro-defining file changed, has a macro call resolved through another crate, or
reads a module of another crate that changed.
