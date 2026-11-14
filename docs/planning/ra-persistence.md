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

`vendor/` holds ten `ra_ap_*` crates at 0.0.358, wired in through `[patch.crates-io]`.
`vendor/ra_ap.patch` is the whole difference from the published sources (about 240 changed lines;
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

## Status of the work

1. Vendor the patched crates; build and test cordial against them. **Done.**
2. Make item identity stable across processes. **Done.**
3. Save the database and restore it in the next run. **Done**, see below.
4. Persist cordial's queries, with stable keys.
5. Measure on a multi-crate workspace.
