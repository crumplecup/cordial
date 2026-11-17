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

## Procedural macros: the same macro in the next process

A persisted memo's flattened dependencies include `ProcMacros.by_crate` whenever its crate expands a
derive or attribute macro (a definition map reads it through `get_for_crate`, and the wrapper query
that shields in-process callers is not part of the saved edges). The loader wrote that input on every
load, with expanders that are handles to a server started in that process, so a restored memo that
reached it was stale before it was asked for. On cordial every one of the first ingredient's 1,258
memos reached it. The specimens had no proc macros, so none of the tests showed it;
`tests/saved_proc_macros.rs` does (a workspace with a derive crate and a user of it, run cold and then
restored).

The fix is an identity for each expander, and a comparison before the write:

- `ProcMacroExpander` gains `identity()` (the dylib's path, its modification time and size, the macro's
  kind and name; the same in every process that loads the same dylib) and `attach()`.
- The saved `ProcMacros` is no longer empty: per crate, each macro's name, kind, disabled flag and
  identity, or the loading error. A restored macro is a `RestoredExpander` that knows its identity and
  forwards to a live expander once one is attached.
- `ProcMacrosBuilder::build_in` compares what it built with what the database holds. When every crate's
  macros are equal (by identity), it attaches the live expanders to the saved ones and writes nothing.
  When one differs (a rebuilt dylib), it writes, and Salsa invalidates what expanded a macro.

What this does not do yet: the server is still started and the dylibs loaded on every run. With the
identity known from a stat of the dylib's path, both can wait until something expands a macro.

## The durability watermark

A persisted memo's flattened dependencies were about 8,000 inputs on cordial, almost all of them the
library's (file texts, source roots) and the crate graph's (crate data, cfg, env). Salsa already has the
idea that makes these cheap to check: an input is written with a durability, and `last_changed_revision(d)`
says when an input of durability `d` or higher last changed. The vendored Salsa now saves one edge, the
watermark, for every input of MEDIUM durability or higher a memo reads, and verifies it as "has any such
input changed since this memo was verified". The inputs that remain as edges of their own are the
workspace's files (LOW). Measured on cordial: about 1,930 edges per memo instead of 8,000.

The cost is imprecision: any change to a library file or to the crate graph makes every restored memo that
read one stale, not only the memos that read the changed input. That is rare, and a run that finds it is a
cold run, which is what it would have been.

What this needs from the rest of the system: nothing may write a MEDIUM or higher input during a run that
restored a state (it would make every restored memo stale). The loader compares before it writes
(source roots, file texts, the crate graph, proc macros), and the crates and file copies a cfg context adds
are LOW durability for this reason.

## What a save costs, and when it happens

The flattening of dependencies (see the Salsa section) is now shared twice over. Each non-persisted
query's persisted edges are a bitset over the edges seen, and equal sets are the same set, so unioning
the callees of a query is a few word operations. A persisted memo does not list its edges: it refers to
the sets its non-persisted callees reach and lists its own few. The file holds each distinct set once,
in a `sets` entry at the head of the ingredient, and the memos refer to them by number; a memo restored
has them written out again (it keeps its own copy in memory).

A session saves once, at the end of its first projection, after the contexts and the leaf pass; a run
that fails in them still saves what it computed. A session restored from a state saves nothing if no
persisted query has executed in the process since it began (`salsa::persisted_executions`, counted where
a function memo with `persist` executes): what it would write is what is there. It says so
(`analysis state: nothing new to save`).

## A function is described by where it is, not by its byte offset

A description used to carry the byte offset of the item's name, so a comment added above a function
changed the description of every item below it. It now carries a *place*: the labels of the enclosing
modules, impls (trait and type), traits and functions, then the item's own (`fn new`, `impl Make for
A`, `call my_macro`), outermost first, with `#n` after a label when `n` earlier siblings have the same
one (two functions of one name under different `cfg`s). Items a macro generated are the place of the
outermost call plus the offsets inside the expansions, which do not move with text above the call.
`resolve` finds the item again by walking the file for the last label and comparing places.

This does not by itself keep a function's saved evidence after an edit to its file: a restored memo's
dependencies are flattened to the inputs underneath, among them the text of the crate's root file, and
editing any file makes every function of the crate be evaluated again (measured: a comment in `lib.rs`
evaluates the functions of `other.rs`). What would: evidence without absolute positions, edges kept to
a few intermediate queries (the def map, a file's item tree) with a fingerprint of their output so a
restored run can re-run them and stop at an unchanged one, and a cutoff in front of each function that
reads only its own text and what it resolves through. See the staged plan below.

### One evaluated function in a restored process costs the trait solver's whole index

Measured on a specimen and on `elicit_doc`: a restored run that evaluates nothing executes about 27,000
queries; one that evaluates a single function executes about 125,000, 73,000 of them `capability`
(`impls_trait`), which builds the impl index of every crate including the sysroot (about 30,000 impl
signatures). So the functions a restored run cannot find again cost far more than their number.
The per-phase report (`cordial::phase_queries`) showed it: 290,677 queries in the restored workspace
load of `elicit_doc` (490,315 cold), with `ImplSignature::of_` and its companions near 30,000 each.

The 170 functions `elicit_doc` evaluated again were all in `tests/support/mod.rs`, a file every test
binary includes with `mod support;`, so it is one file in many crates. Its description (place and
path) was the same in each crate and `resolve` found the first crate's function. A definition whose
file is in more than one crate now carries the root file of its crate in the description and is found
through that crate's index (`generated_in`).

### Naming must not be quadratic in a file's items

The first place-based description counted the earlier same-label siblings of each item one by one,
and found an item again by scanning for its label and recomputing the places of every candidate: time
in proportion to the square of the items in a file when many share a label (thousands of `impl S {}`
blocks). Restoring one file of 4,000 impls took 68 s, against 12 s now. A file is walked once
(`PlaceIndex`): each parent keeps its own counts, the result is indexed both ways and kept by Salsa
until the file's text changes. `the_time_to_name_functions_does_not_grow_with_the_square_of_a_files_items`
fails on the old algorithm (125 s for 3,000 impls against 20 s for 1,000) and passes on this one. A small
quadratic term remains (about 0.4 µs per n squared: 24 s of 30 s at 8,000 items in one file).

On `elicit_doc`, restored: 0 of 2,142 functions evaluated, 108,264 queries (290,677 before the
shared-file fix), 26 s: 5 s to restore and load, 8 s to name each function and find its memo, 8 s to
build the graph from the evidence, 5 s for the contexts and leaf passes.

### What cordial's restored run showed next

With the shared-file fix cordial's restored run evaluated 90 of 20,411 functions (660 before) and 10
per leaf pass (0 before), and was stopped by its memory limit (12.4 GB, 75% of the 16.5 GB available)
in context 1. The 190 functions it could not find again had two causes of ours:

- A derive's expansion has the `derive(...)` meta node of its attribute as the call it came from, and
  the place index labelled items only, so every derived item in a file had an empty place and two types
  deriving the same macro collided. Attributes and their meta nodes are labelled now (`attr derive`,
  `meta derive`); a debug event (`cordial::semantic::identity`, "a node with no place") names any node
  kind that still misses.
- A file in several crates (every file of a library that also builds its unit-test target) was resolved
  only through the crate's index, which lists no item declared inside a function body. The direct route
  runs first again and keeps a candidate only when its own description, which names the crate, matches.

### Where a restored cordial run's time goes (hotpath report, spans on the low-volume modules)

Measured with `--features full,hotpath`, `RUST_LOG` at debug for `contexts`, `context_crates`, `prime`,
`saved`, `session` and `projection` only, and `CORDIAL_LOG_SPANS=close`, 235 s in all:

- `add_context_crates` (6 calls, one per context and leaf pass): 104 s, 44%, all of it `counterparts_of`.
  The leaf passes themselves (`read_leaves`, 5 calls) are 4 s; their 20 s each was this. Inside it
  `declared_in` (3,965 calls) is 34 s and `place_in_file` through the place index (684,000 lookups) is
  70 s, so building an index for each copy of a file dominated.
- `SavedState::restore`: 74 s, 32%, one call.
- `package_sources`: 21 s, once. `local_functions`: 7 s. Evidence priming: 15 s.

Debug spans of the hot modules (`item_key`, `queries`) must not be logged: they were 75 MB/s and inflated
the run from 235 s to 451 s. The per-span counts are in the hotpath report instead.

The index labelled every attribute; only `derive` is a call, so only it has a place now, and the index
builder is measured. `counterparts_of` itself matches every definition of every context copy to the
ordinary build's by place. The place is in the description, so the counterpart of a copy's item is
the same description with the original's path: it could be computed per item when asked, and checked
to exist, instead of by listing every definition of every copy.

### Staged plan toward per-function reuse within a file

1. Place-based description (done): `a_comment_above_a_function_does_not_change_what_describes_it`.
2. Evidence free of absolute positions (done): a call's range is kept relative to where its function
   starts (or its outermost macro call, for a generated one) and made a file coordinate again where the
   evidence enters the graph (`queries::in_file`), so the consumers are unchanged. Measured first: a
   comment above a function moved only `calls[*].range`, and in a live process `ordinary` (no calls)
   was not evaluated again while `caller` was; now neither is
   (`an_edit_evaluates_only_the_functions_it_changes_in_the_process_that_made_it`). This holds in the
   process that made the edit; after a restore the flattened dependencies still reach the file's text.
   Measured limit: `evaluated` counts `collect_reachable` only. `direct_facts` (the part that reads the
   function's syntax and resolves through `sema`) still runs again for every function of an edited
   file, in the live process too, because it reads the file's whole syntax tree. Avoiding that needs
   a dependency that is the function's own text and what it resolves through and not the file's tree;
   Salsa records every read, so it takes either a body-level walk over rust-analyzer's `Body` and
   inference (which have cutoff in-process) or a primitive that runs the heavy part with dependency
   recording suspended behind a fingerprint, which is sound only if the fingerprint covers
   everything the part resolves (callee signatures, imports, the impl's header).
3. Salsa: fingerprinted edges to selected unpersisted intermediates, verified by re-running them, and a
   per-function cutoff query. Needs a check first that rust-analyzer's def map and item tree outputs
   can be fingerprinted, and an in-process test that editing one function leaves another's evidence
   unevaluated at all.

## A persisted memo is saved as of now

A memo is saved with its dependencies flattened from what the queries it reads depend on *now*, and
with the revision it was last verified at. If a query it reads has run again since (asked on its own,
or because the slot of a reusable interned key was given to a new value, the way a `FunctionKey` is
when the contexts intern their functions), the memo is still valid in the process but its saved
dependencies are the other query's current ones, and the next process finds it out of date. On cordial
this was about 8,000 ordinary functions of the vendored crates, "stale" because of inputs of the
contexts' copies, which did not exist when the functions were computed.

`dyn Database::verify_persisted_memos` verifies every persisted memo at the current revision, running
again the queries whose dependencies changed; cordial calls it before it saves. The cause was found
with events at the source (`salsa::stale`, `salsa::write`, `salsa::explain`, with `cordial::tables` and
`cordial::keys` to say what the numbers are) and `tests/salsa_flatten.rs` holds the minimal case:
`outer` (persisted) reads `middle` (not), `middle` runs again onto another input, and the restored
`outer` runs again unless it was verified before the save.

## Functions macros generate

A description is the site of a definition's name: the file's path, the offset, and, for a generated
definition, the chain of offsets through each expansion. Reading one back found a definition written
in the file, or a function one attribute macro wrapped (the annotated function sits at the call).
Everything else came back "not found" and was evaluated again in every run: a derive's methods, a
function two attribute macros wrapped (`#[instrument]` and `#[hotpath::measure]`, which cordial puts on
nearly every function), what a `macro_rules!` call defined. `tests/saved_generated.rs` had 7 of its 11
functions in that state.

- **An index of what was generated.** `generated_in(crate)` lists the crate's definitions whose site
  has an expansion chain, by description (the crate's modules and the associated items of its impls and
  traits, which is where a derive's methods are). `resolve` tries the cheap reads first and falls back
  to the index. It is a tracked query, so it is out of date when the crate's definitions are, which a
  persisted memo already depends on.
- **Built-in derives.** `Clone::clone` of a `#[derive(Clone)]` has the trait's own method (in `core`) as
  its source, the same for every type, so every such function had one description and none could be
  found. A function whose source is in another crate than its own is described by the site of the type
  its impl is for, plus `Trait::method`. The description has a member field for it:
  `kind|route|offset|chain|member|path`.
- **What it cannot do.** A proc macro that produces different text in different runs gives different
  offsets, and so a different description: its functions are evaluated again, which is correct.

## A context depends on its own crates and nothing of another context's

Adding a context used to write one input that listed every context's crates, and every memo about
any context's files read it (`source_root_crates` for a tool source root). So each new context made
the memos of all the earlier ones stale. They were saved stale, because nothing asked for them again
before the save, and a restored run computed them again: a specimen with two contexts over two
dependent local crates showed it, and on cordial (five contexts) every context but the last was
recomputed.

A context's crates are now a field of that context's own source root input (`SourceRootInput::crates`,
empty for the roots the loader makes), and a lookup of a tool root's crates reads that root. A small
registry (`ToolRoots`, read only by `all_crates`) lists the tool roots. The path-to-file lookup
(`file_at`) looks in the one source root a path can be in, in the order of the ids, instead of in every
root in hash order: it used to read the file set of every context's copy root to find an ordinary file.

The restored run reports what made memos stale, by input (with examples named) and by query, after the
ordinary pass and again at the end: `analysis state: restored memos found out of date ...`.

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

## A restored run builds no definition map, and its memos share their sets

Three findings from the restored run's profile, each with a guard in `tests/`:

- **Naming a function needs no rust-analyzer when nothing changed.** What asked rust-analyzer for
  definitions in a restored run, and so built the definition maps of the crate and the sysroot
  under it (about 28,000 queries, 5,661 macro expansions), was the enumeration of the workspace's
  functions, a file's edition (`attach_first_edition` finds its module), the planning scan and the
  merge check, the files of a crate's modules (`package_sources`, `compiled::collect`), a
  function's external location and trait, and each context's walk of its copy of every file. Each
  is now a saved query keyed by a description that means the same in every process:
  `crate_functions`, `crate_files`, `functions_in_copy`, `canonical_description`, `start_of`,
  `implemented_declaration`, `crate_declares_a_name_twice`, and `edition_of` reads the crate's
  input instead. A restored run over the specimens executes under 40 queries, from about 52,000.
  After an edit the crate that changed still builds its maps and the sysroot's, which is the
  remaining fixed cost, and sharing them across processes would need rust-analyzer's definition
  maps to be serializable.
- **A context's copy of a file has its original's places** (`PlaceIndex` is built once for both),
  and a copy's counterpart in the ordinary build is found from its description, not by listing
  every definition of every copy.
- **Restored memos refer to the sets they share and do not copy them.** The saved state already
  held each set of edges once, but decoding copied the whole set into every memo that refers to it:
  a memo in a crate of `n` files was given `n` edges, so the edges, the decode time and the memory
  grew with functions times files (187,673 edges for 400 files, 969 ms, from 72 for none). A
  restored memo now holds one edge that names the set (`QueryEdge::set`, beside the watermark
  edge). Verifying it checks the set's edges once for all the memos that share it, for each
  revision and each revision the memo was verified at, and keeps the answer only until the database
  next changes. Saving a restored memo again saves it as referring to the same set, so a state
  saved again is no larger than the one restored (it was three times as large). The tests choose
  what changes (a cell of a set, of another set, the library input, nothing, in turn, after one
  save and restore or two) and expect exactly the queries that read it to compute again.

Measured with `salsa::edges_expanded()` and the `decoded the saved Salsa memos` event (its `edges`
and `micros`), and `salsa::database=debug` for each ingredient's decode time.

### Measured on cordial (2026-11-16, release build with hotpath)

A restored run of `cordial open --stop-after source_ir`, nothing changed since the cold run:

| | before | after |
|---|---|---|
| wall time | 235 s | 31.9 s |
| `SavedState::restore` | 74 s | 12.2 s (read 0.15 s, decode 12.0 s) |
| queries executed loading the workspace | 237,976 | 2,563 |
| functions evaluated (workspace and leaf passes) | 0 | 0 |
| fallbacks to rust-analyzer | not counted | 0 |
| peak memory | 12.3 GB | 7.8 GB |

The cold run took 387 s and saved an 800 MB state (`save_state` 101 s, 26 s of it the write). What
the state holds is mostly sets: `persisted_evidence` is 29,362 memos over 8,422 distinct sets of
about 3,300 edges each (27.9 million), and the other ingredients add about 22 million. Decoding
them is the restored run's largest remaining cost: `persisted_evidence` 4.7 s, `file_items` 1.8 s,
`functions_in_copy` 1.1 s, `crate_files` 0.8 s. Restored memos were given 6.3 million edges of
their own; copies of their sets would have been tens of millions.

To measure a restored run the run cache must be out of the way (`rm -r <store>/cache/run`), or the
run reports the cached IR and restores nothing.

## Cutoff-aware flattening: reuse after an edit

Flattening replaces everything a memo read with the inputs it reached. A function that resolves
one name reads its crate's definition map, which is made from every file, so every function of a
crate depends on every file of it: one edit to a function's body makes the evidence of every
function of the crate stale, though the definition map the edit makes is the one it had. In
process, Salsa cuts that off, because the map's value is the same; the saved state loses it. A
specimen (a crate of 6 files of 5 functions, nothing calling across files) computed evidence for
all 30 functions, and one more, after one function's body changed.

**The mechanism (done, in `vendor/salsa`).** `register_cutoff(query, fingerprint)` declares that
the persisted query `fingerprint`, which takes the same argument as `query` and returns something
that determines `query`'s value, stands for it. Where the flatten walk would pass through
`query`, however it reaches it (read by a memo, or through a tracked struct the query made), a
memo is saved as depending on `fingerprint` of the same argument instead. After an edit
`fingerprint` is verified as any query is: it runs again, `query` is recomputed, and a value that
is the same does not change, so what depended on it is still valid. The walk goes through `query`
when the fingerprint has not been asked for the argument, and the fingerprint's own memo always
goes through the query it fingerprints. The tests (`tests/salsa_flatten.rs`) choose each of
registered or not, an edit that leaves the fingerprinted value or changes it, one save or two,
JSON or MessagePack, a fingerprint asked for or not, a value read directly or through a tracked
struct, and expect exactly the queries that read what changed to run again.

**The fingerprint (done, in `vendor/ra_ap_hir_def` and `vendor/ra_ap_hir_expand`, `stable` modules).**
A hash of a crate's definition map and extern prelude that means the same in every process. A
definition map is made of ids that a process numbers in the order it meets them, so a hash of its
bytes says nothing across processes, and a type that holds an id is never hashed by its derived
`Hash` (another process may give the same number to something else, and the hashes would be equal
for things that are not). Each type has an impl that says what its ids stand for: an item by where
it is written (its file's path and its place in the file) and what it is, a module by its crate, its
block and its names, a macro call by its definition and call site. Every impl takes its type apart
without `..`, so a field rust-analyzer adds later is a compile error until it is accounted for. Two
things are left out, each named at the place it is: what a diagnostic says (nothing that computes a
function reads it; which kind and where are hashed), and the arguments a macro was called with (what
they expand to is in the items it makes, as names and places). What has no meaning that survives the
process makes the hash volatile, which is safe (it differs in every process, so nothing is reused)
and is counted, never absent.

**Why it can be trusted, and what keeps it so.** `tests/edit_reuse.rs` is a differential battery: for
nine kinds of edit (a comment, a body, a return type, a function moved to another file and
re-exported, a visibility narrowed, a rename, what a macro defines, a method's return type, a
function gated off) a run restored from the state saved before the edit says what a run from
nothing says on the edited sources, and the edits that must change what is said change it. The move
is the case a hash of rust-analyzer's own dump of the map fails: the dump names what is in a module
and not where it is written. The option `reuse_across_edits` (off by default) turns the cutoff on;
it stays off until a run on real code has been clean.

**What it buys.** In the independent-files specimen (6 files of 5 functions) an edit to one
function's body computes the evidence of the functions of its file and the warm-up's query, 6 of 31;
without it, all 31.

**First run on cordial (`reuse_across_edits = true`, a one-line edit to one function's body).** The
audit computed again 260 of the 5,975 functions the run reused: none differed. No definition map was
volatile (118 were, before a macro call's hygiene context was given a meaning that survives the
process), and nothing fell back to rust-analyzer. The reuse was partial: 14,748 of 20,723 functions
were computed, so 29% were reused. The flatten report's `breadth` and `reads_themselves` (and, in a
test, `salsa::flatten_breadth`) say why: the kinds of query that read many leaf edges themselves
are `TraitImpls::for_crate_and_deps_` (one query reading every file's impls, so every function that
resolves a trait or a method depends on every file), cordial's own `resolve` (which finds a file by
scanning the source roots, so every function depends on all of them), and `crate_local_def_map`
(the one cut). A specimen of 6 files with an inherent method and a trait impl each, and functions
that call both, reproduces it: after an edit to one body, 39 of 42 functions were computed.

**The impl indexes.** The flatten report's `breadth` (the widest query of each kind, and which
kinds of leaf its set holds) named `InherentImpls::for_crate_` and `TraitImpls::for_crate_` as the
widest queries after the evidence ones, each reaching the text of every file: every function that
calls a method depended on all of them. `ra_ap_hir_ty` is vendored for this (`method_resolution/
stable.rs`: the two indexes taken apart without `..`, `SimplifiedType` and `SolverDefId` matched
exhaustively, the ids the solver numbers itself volatile), each index has a persisted fingerprint
in cordial, and the two `for_crate_` queries are cut to them. In the 6-file specimen with impls an
edit to one body now computes the functions of its file and no others (it was 39 of 42). The
battery has four edits that change the indexes (a trait impl or an inherent impl moved to another
type, a trait impl added, a trait method's body) and all agree with a run from nothing.

The battery found something that was not the cutoff: two runs of the same sources put a crate's
function nodes in a different order in the graph's `by_kind` index. `connect` set an attribute on
each node in a hash map's order, and every `set_attr` indexes the node again. Ordering by item id
did not fix it (an id is the order its key was interned in, which differs between a restored run
and one from nothing); it now goes in the order the nodes were made. Setting an attribute still
adds the node to `by_kind` again, so the lists hold duplicates: noted, not changed here.

**The first performance evaluation (2026-11-16, `cordial perf workflow --repeat 1`, commit
`3956a8d`, `reuse_across_edits = true`).** Cold 621.7 s (`source_ir` 591 s, against 387 s in the
earlier cold run: not explained yet); warm 23.8 s (run cache); the incremental state, after a
comment appended to `src/perf/spread.rs`, was stopped by the memory limit during "Saving the
analysis state": 13m59s, 6.9 million queries (a cold run executes about 4 million), 21.9 GB
against a limit of 21.7 GB. A cfg context was also cut by the memory budget in the cold run and
in this one. The workflow threw away the cold and warm records because the last state failed, and
the stopped run wrote no metrics; both are fixed (`measure_workflow_partial`; `write_ended`).

**Why an edited run costs a cold run, reproduced at specimen scale** (`edit_reuse`, a crate of 6
files with a feature-gated function in each, so that the planner adds a cfg context): after an
edit to one body, the restored run executed 125,721 queries and the run from nothing 126,154. Only
8 of them were functions' evidence. The rest are rust-analyzer's own queries over the dependency
crates (`AttrFlags::query_`, `ImplSignature::of_`, `impl_self_ty_with_diagnostics`,
`impl_trait_with_diagnostics`, about 12,400 each), asked by `capability` in `recording.rs`:
`ty.impls_trait(db, Debug, ..)` is a trait-solver question, and answering it makes rust-analyzer
build the trait impl index of every crate the type's crate depends on, which is every impl of the
sysroot and of each registry crate. None of those queries is persisted, so a restored run that has
to compute any function's evidence pays rust-analyzer's whole warm-up for the dependency layer,
which does not depend on the edit at all. The cutoffs make the *functions'* evidence cheap; they
cannot make the layer under it cheap. In the specimen this is 99.9% of the queries; in cordial,
with its registry dependencies, it is millions of queries and the memory of the whole dependency
graph.

**Persisting the dependency layer: what it takes (explored, nothing built).**

*What the layer is, measured.* The 125,721 queries of the edited run in the specimen, by the step
that asked: under `capability` about 75,000 (`ImplSignature::of_`, `ImplSignature::with_source_map_`,
`impl_self_ty_with_diagnostics`, `impl_trait_with_diagnostics`, each 12,427 times, one per impl of
the sysroot, and `AttrFlags::query_` 12,969, which is the impl index being built); under
`collect_direct` about 23,000 (`AttrFlags::query_` 22,154 and `ImplItems::of_` 1,135); under
`inner_` and the rest about 25,000 (file item trees, ast id maps, macro arguments and expansions of
the dependency files: the first touch of the definition maps). The `collect_direct` ones are not
the function's body: `FamousDefs` and `lang_item` lookups make rust-analyzer's `lang_items` read the
attributes of every item of every crate in the extern prelude chain (`hir_def::lang_item::
lang_items` -> `crate_lang_items` -> `AttrFlags::lang_item`). So about three quarters of the cost
is three crate-wide indexes, `lang_items`, `TraitImpls::for_crate` and `InherentImpls::for_crate`,
whose results are small (ids), and the rest is definition maps and file-level queries.

*What persisting them needs, from a spike (reverted).* Every id comes from `impl_intern_key!` in
`base_db`, and Salsa's `interned` takes `persist`. With it, the first errors name 16 id types whose
`Loc` has no serde: `ItemLoc<N>`, `AssocItemLoc<N>`, `EnumVariantLoc`, `Macro2Loc`,
`MacroRulesLoc`, `ProcMacroLoc`. Deriving those moves the errors one level down: `ModuleIdLt`,
`AstId<N>` (a `HirFileId` and a `FileAstId<N>`), `MacroExpander`, `Name`, `MacroRulesLocFlags`,
`ItemContainerId`, `CustomProcMacroExpander`. Below that is `HirFileId`, and with it `MacroCallId`
and `MacroCallLoc`, which the sysroot's macro-generated impls are written in: its call kind holds
spans, hygiene contexts (`SyntaxContext`, interned in `span`) and, for eager macros, token trees
that have no serde. That is a bounded set (a few dozen types, all in crates already vendored) and a
decision about eager token trees, not an open-ended one. Then `lang_items` and the two impl indexes
get `persist`; the indexes are keyed by `SimplifiedType<SolverDefId>`, whose closure and anonymous
constant ids are interned in `hir_ty` and cannot be in an impl's self type, so they need a
serializer that refuses them. What reads these memos is verified as the rest is: flattened to the
text of the dependency files and the crate graph, which do not change between runs.

*What it would leave.* The definition maps and file-level queries of the dependency crates, about a
quarter of the specimen's cost, which is the first-touch of name resolution in the sysroot and each
registry crate. Persisting definition maps is a larger step (a `DefMap` holds macro expanders and
proc-macro handles) and is not needed to remove the rest.

*Stages, each measured by the reproduction (`an_edit_to_a_body_costs_a_small_part_of_a_cold_run_
when_the_crate_has_a_cfg_context`).* S1: ids persistable, `lang_items` persisted: the 23,000 under
`collect_direct` should go. S2: `TraitImpls::for_crate` and `InherentImpls::for_crate` persisted
for crates that are not workspace members: the 75,000 under `capability` should go. S3, only if the
remainder matters on cordial: file-level queries or definition maps. The test's bound is set from
what S2 measures, and not before.

*S1 attempted, and what it found (nothing committed).* Making the ids persistable is as bounded as
the spike said: `span` (hygiene contexts, `Span`, ast ids; `text-size` with its `serde` feature),
`tt` (`TopSubtree` is a byte buffer, compressed span parts and symbols: it derives), `mbe`
(`ExpandErrorKind`, and `MacroCallStyles` by hand from its bits), `hir_expand` (the macro call
chain, builtin expanders, `InFile`, `HirFileId`) and `base_db` (`EditionedFileId`, and the ids from
`impl_intern_key!`) all compiled with serde and `persist` on their interned structs. What stopped
it is `hir_def::ModuleIdLt`: it is a tracked struct, created by `crate_local_def_map`, and every id
(`ImplId`, `TraitId`, ...) holds one in its loc. A tracked struct's id is found, when its creating
query runs, in the identity map of that query's earlier memo (`zalsa_local.tracked_struct_id`);
with no earlier memo a new slot is allocated. The definition map is not persisted, so in a restored
process it runs again with no earlier memo, and numbers its modules afresh. An `ImplId` in a
restored impl index would then name a module that no longer has that number, and the impl re-derived
from the new definition map would be a different id for the same impl. So persisting the three
indexes alone is not sound, and S1 and S2 as staged cannot stand.

Two ways to make the module ids reproducible, to be chosen: (A) persist the definition maps of the
dependency crates, whose tracked structs and identity maps Salsa already keeps with a persisted
creating query (a `DefMap` is about forty types, the same closure `nameres::stable` takes apart,
and removes the remaining quarter of the cost too); or (B) keep the identity maps of the creating
queries that are not persisted, in Salsa, so that when `crate_local_def_map` runs again it finds
each module's earlier id by its identity (crate, block, parent, name), which is what its earlier
memo would have given it. (B) is smaller and leaves the definition-map first touch to recompute.

**Still to do.** The dependency layer, as above. Before it: persist it (rust-analyzer's impl indexes and what
they read, for crates that are not workspace members, whose inputs do not change between runs; the
stable ids of `hir_expand::stable` are the means), or stop asking the trait solver for facts that a
saved answer holds. Then: measure the cordial run with the impl fingerprints; cordial's `resolve` should find
a file without reading every source root if it still shows in the report; the walk-width metric names them. Functions of one file still all read the file's
syntax, so the next granularity is a function's body. The audit that recomputes a sample of reused
functions and compares them, so that a construct the battery does not think of is caught in the
field, is not written.

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
