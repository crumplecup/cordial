# rust-analyzer persistence: reuse across processes without a resident process

**Status:** Experiment, 2026-11-14. The patched crates are vendored and cordial builds against
them. Item identity is stable across processes, and a run saves its database for the next one
to start from. Cordial's own queries are not persisted yet, so what carries over is rust-analyzer's
inputs and the identity table.

## Why

A resident process that keeps the loaded workspace between commands measured about 10 GB, so
none may exist (see `incremental-cache.md`, C3). Reuse across commands then has to come from
state saved on disk. Salsa 0.28.5 has an experimental `persistence` feature that saves memos
with their dependency edges and revisions, and validates them against restored inputs: the
red-green algorithm, made durable.

## What was patched

`vendor/` holds eleven `ra_ap_*` crates at 0.0.358, wired in through `[patch.crates-io]`.
`vendor/ra_ap.patch` is the whole difference from the published sources (about 315 changed lines;
the copied crates are 106k lines of Rust, 3.7 MB, none of it ours to maintain):

- the nine `#[salsa::input]` structs rust-analyzer defines are marked `persist`:
  `LibraryRoots`, `LocalRoots`, `FileText`, `FileSourceRootInput`, `SourceRootInput`,
  `AllCrates`, `Crate` (`ra_ap_base_db`), `ProcMacros` (`ra_ap_hir_expand`) and
  `ExpandProcAttrMacros` (`ra_ap_hir_def`);
- serde on the data they hold (`CfgOptions`, `Env`, `FileSet`, `VfsPath`, `Symbol` as its
  text, `Edition`, the crate data types); `FileText` is saved as a plain string and
  `ProcMacros` as empty, since loaded expanders cannot be saved;
- `RootDatabase::empty()`, because `new` allocates singleton inputs that a restore must not
  find already there, and `Files::dump`/`restore` and `CratesMap::dump`/`restore` for the
  handle tables rust-analyzer keeps outside Salsa;
- `Vfs::interned_paths`, and `load_workspace_into_db_with_vfs` in `ra_ap_load-cargo`, so a load
  can start from a VFS that already knows some paths (see "stable file ids" below);
- compare-before-set in `Files`' setters and in the local and library root sets, because Salsa
  does not compare a new input with the old one and an unchanged value must not invalidate what
  read it.

Only inputs need to be persistable. Salsa flattens a memo's edges through non-persistable
queries, interned structs and tracked structs down to the base inputs.

On a rust-analyzer bump: copy the new crates, apply `vendor/ra_ap.patch`, and fix what no
longer applies. The change is meant to be offered upstream.

## Measured (a two-file crate, one persisted query reading its def map)

| change between save and restore | executions |
|---|---|
| none | 0 (about 76 µs) |
| a comment appended | 1 |
| a body edit | 1 |
| a signature edit | 1 |

Reuse is exact and sound, with no hand-declared dependencies, and it is **per crate**: a
memo's edges flatten to every file's text, because name resolution reads them all. Below that
granularity needs rust-analyzer's intermediate queries (item tree, signatures, def map and the
interned ids in them) persisted too, so that a body edit is found equal at the item tree and
stops there. A probe on `file_item_tree_query` alone needed serde on about 40 types and then
reached the interned `EditionedFileId`, `MacroCallId` and `SyntaxContext`; the signature
queries would follow. That is upstream-sized and is not started.

## Item identity (done)

`SemanticItemId` used to be an index into a table the session interned definitions in, so it
meant something only until the process ended, and every fact cordial computes carries them
(`SemanticFunction`'s definition, call targets and error types; `FileItems`). A memo restored in
another process would hold numbers meaning other items, and a partial reuse would compare ids
from two tables.

It is now the id of an interned *description* of the definition (`loader/database/item_key.rs`):
the kind, the file's path in the database, the offset of its name, and for a definition a macro
generated the chain of expansions out to the real call. The same definition has the same
description in every process, so a restored table gives it the same id. The table is immortal
(`revisions = usize::MAX`), so an id is never reused. `definition(id)` finds the item again
from its description, and refuses to return anything that does not describe itself the same way.

- Tests found two real defects while this was written: macro-generated definitions from two
  invocations of one macro shared a description (so the chain was added), and attribute-macro
  functions such as `#[instrument]` ones were not resolvable (so they resolve through the
  annotated function).
- A function-like macro's generated items are unique and stable but cannot be found again from
  their text; a fact about one reads back as an unknown definition, never a wrong one.
- `SourceDatabase::describe_item` and `knows_item` expose this for tests and diagnostics.

## Saving and restoring (done)

`loader/database/saved.rs`. A run saves its database once, at the checkpoint between the ordinary
projection and the cfg contexts, into `{store}/cache/analysis-state/` (one slot, replaced). The
contexts are left out on purpose: each run makes their shadow crates and copied files fresh, and
saving them would pile up orphans. The next run restores that state into `RootDatabase::empty()`
and loads the workspace onto it; the loader's changes then bring the inputs up to date, and
Salsa invalidates what a difference reaches.

- The state is the Salsa data (rust-analyzer's inputs, the identity table), the handle tables
  rust-analyzer keeps outside Salsa, and a key: the fingerprint of the cordial binary that wrote
  it. Another build's state is ignored, because the facts and descriptions in it are that
  build's. It is written beside the slot and moved into place with the key last.
- Any failure reading it (missing, wrong build, damaged, a panic inside Salsa) is said in the run
  log, for example `analysis state: not used (saved by another build of cordial); loading cold`,
  and the run goes on cold. A saved state never decides whether a run succeeds.
- Size: about 57 MB for a two-function project, nearly all of it library file text. Space is
  traded for time. Saving took 84 ms there; neither number is measured on cordial itself.
- `SourceDatabase::load_saved` and `restored()` expose it; the run's deferred load uses it
  whenever the run cache has a key.

## Stable file ids (found by a failing test)

Salsa's inputs are keyed by `FileId`, and the VFS hands those out in the order files are first
seen, which with the loader's threads is not the same on two runs: a restored run found file
27's inputs describing a different file and overwrote most of them, invalidating everything.
The saved state therefore also carries the VFS's paths in id order
(`Vfs::interned_paths`, deleted ones included), and the next load seeds its VFS with them
before loading (`set_file_contents(path, None)` allocates an id and records no change), so every
known path gets the id it had and new paths follow.

## Persisted function evidence (done)

`persisted_evidence(db, ItemKey)` in `loader/database/queries.rs` is a `persist` query keyed by a
function's description, computed by the process-local `function_evidence`. Every lookup goes
through `evidence_of(db, function)`, which uses the persisted query when the description finds
that same function again and the process-local one when it does not (a function a macro
generates). So a restored database answers from the saved evidence without computing it, for as
long as no input it read has changed; any edit to the crate's sources recomputes it, the
per-crate granularity above.

Measured on the test specimen: a cold run evaluates all five functions; the restored run
evaluates only the two macro-generated ones; after an edit to one file, what read it is
evaluated again. What this buys: evidence survives anything that does not change what
rust-analyzer read: a policy or configuration change, an edit to a fixture or a document, an
edit in another crate of a workspace.

## Salsa's serialization, and the patch to it

Salsa 0.28.5 rewrites each persisted memo's dependencies before it is saved: every query that is
not itself persisted is replaced by the persisted inputs it reads (`collect_minimum_serialized_edges`
in `function.rs`). It does that by walking the non-persisted queries from each persisted memo and
clearing its visited set between memos, so the cost is the number of persisted memos times the size of
the graph each one reaches. On the specimen crates that is nothing; on cordial (about 19,000
function-evidence memos plus every per-file query over a graph of millions of rust-analyzer queries)
the save did not finish in ten minutes. It looked like a hang: no query ran, memory crept up, and the
phase message was still the last one set.

`vendor/salsa` is Salsa 0.28.5 with the walk done once per query (`vendor/salsa.patch`, about 115 changed
lines in `function.rs` and a new `flatten.rs`; the rest is the manifest losing its test targets):

- `EdgeFlattener` keeps, for each non-persisted function memo, the persisted edges it reaches. The
  hook's `visited_edges` parameter became the flattener, so six `Ingredient` implementations changed
  one parameter type.
- A query reached again while it is still being walked sits on a cycle; its result is incomplete until
  the head of the cycle finishes, so it is not kept (the lowlink rule). Everything else is exact.
- A result equal to its biggest callee's shares that callee's storage.
- A persisted function reached through a non-persisted one is now a leaf edge (a dependency on its memo)
  instead of being walked through; its own memo covers its inputs. That is smaller, and sound.
- The flattener lives for one function ingredient's memos, so each persisted query family walks the
  shared graph once (about 25 walks, not 25,000).
- Each ingredient logs one `salsa::persistence` event: memos, edges, queries walked, reused, shared,
  incomplete, edges handed out, time. `RUST_LOG=salsa::persistence=info` shows them.

`tests/salsa_flatten.rs` pins it: forty persisted memos over a chain and a cycle of queries that are not
persisted walk the graph once between them, a restored database reuses every memo, and changing the input
recomputes every memo, so the flattened edges still cover what the queries between them read.

## Persisted cfg contexts (done)

A context is shadow crates and copies of the workspace's files (virtual paths, source roots from
`TOOL_SOURCE_ROOTS` up), added by `add_context_crates`. They are now part of the saved state, and
a restored run takes them up instead of making them again: `contexts` in the state records, per
context, its crates (as positions in `all_crates`) and its file copies, and the VFS seed carries the
copies' paths so their file ids are the same.

The problem was not what to save but what saving did to the other memos. A persisted memo's edges
are flattened to base inputs, so a memo that read `AllCrates` is stale as soon as `AllCrates`
changes, and adding shadows changed it after the ordinary memos were computed: every per-file query
re-ran in the next run. Backdating cannot help there, because the intermediate rust-analyzer query
that would have absorbed the change (`source_root_crates`) is not in the saved edges.

So tool-owned crates live in an input of their own, `ToolCrates`, and `source_root_crates` reads it
only for a tool source root; `all_crates` is the graph's crates then the tool's, and `set_in_db`
compares against the graph's only. Nothing about the graph's own roots reads `ToolCrates`, so
adding a context leaves every ordinary memo valid. `a_restored_run_takes_the_contexts_up_again...`
checks that: the only queries a restored run executes are the five joins.

On the next run a kept context is reconciled when asked for: its copies' text and source root,
and each shadow's data, cfg, env and the rest, are written only where they differ from this run's
sources (Salsa does not compare an input to its old value), and shadows whose original crate is
gone are dropped. The state is now saved at the end of the first projection, after the contexts
and the leaf pass, so their evidence is in it.

## Persisted per-file collectors (done)

The per-file queries the projection runs are `persist` queries: `file_facts`, `file_items`, the
`error_*_file` and `observability`, `visibility`, `policy`, `verus` and `error_apply` collectors,
the antipattern facts, walks and contracts, the panic proof graph parts and scans, the
architecture and signature parts, the path-inclusion targets and the tracing inventory. Their
keys are interned structs of a file id, a path and strings, which are stable now; their values are
the IR types they always produced, with serde added where it was missing (and `TextRange` saved
as two offsets). In a restored run on an unchanged tree none of them executes
(`a_restored_run_computes_none_of_the_per_file_collectors`).

- **The joins are not persisted.** `panic_graph`, `architecture_catalog`, `signature_index` and
  the like are cheap and re-run from the saved parts, which also keeps the rule below simple.
- **Format.** MessagePack, not a compact non-self-describing one: the IR nodes carry
  `serde_json::Value` attributes, which can only be read back from a format that says what a
  value is. Salsa reads its map keys as borrowed strings, so it is read from a byte slice.
- **A Salsa limit, worked around.** A tracked function is set up the first time it is called in a
  process, and verifying a restored memo checks the memos it depends on, so a persisted query that
  depends on another persisted query not yet called would panic ("cannot be accessed before
  calling `init`"). Only `file_items` and the tracing inventory do (on `file_facts` and
  `persisted_evidence`); `warm_persisted_dependencies` calls those two once after a restore. A
  persisted query gaining such a dependency without being warmed fails the restored-run test.
  The proper fix is in Salsa (initialise every function ingredient's view caster on
  deserialize), and belongs in the upstream conversation.

## What is not persisted, and why

- **The cfg contexts and the leaf pass are persisted** (see the next section).
- **A workspace with several targets** saves at the first target's checkpoint, since a later
  target's ordinary projection happens after an earlier target's contexts have added shadows.
  Function evidence for every local package is primed once and is in that save; the later
  targets' per-file collectors are not.
- **Stale saves.** A save that died leaves `analysis-state.writing-<pid>`; the next save
  removes those older than ten minutes. `cordial clean` removes the whole cache directory,
  the state with it.

## Status of the work

1. Vendor the patched crates; build and test cordial against them. **Done.**
2. Make item identity stable across processes. **Done.**
3. Save the database and restore it in the next run. **Done**, see below.
4. Persist cordial's queries, with stable keys. **Mostly done**: function
   evidence and the per-file collectors; the joins, the call graph and the contexts are not.
5. Measure on a multi-crate workspace.
