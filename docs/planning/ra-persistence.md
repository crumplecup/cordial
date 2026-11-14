# rust-analyzer persistence: reuse across processes without a resident process

**Status:** Experiment, 2026-11-14. The patched crates are vendored and cordial builds against
them. Nothing saves or restores a database yet; the blocker is item identity (below).

## Why

A resident process that keeps the loaded workspace between commands measured about 10 GB, so
none may exist (see `incremental-cache.md`, C3). Reuse across commands then has to come from
state saved on disk. Salsa 0.28.5 has an experimental `persistence` feature that saves memos
with their dependency edges and revisions, and validates them against restored inputs: the
red-green algorithm, made durable.

## What was patched

`vendor/` holds ten `ra_ap_*` crates at 0.0.358, wired in through `[patch.crates-io]`.
`vendor/ra_ap.patch` is the whole difference from the published sources (224 changed lines;
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
- compare-before-set in `Files`' setters, because Salsa does not compare a new input with the
  old one and an unchanged text must not invalidate what read it.

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

## The prerequisite: item identity

`SemanticItemId` is an index into an interned table of the session, "meaningful only inside its
analysis session". Every fact cordial computes carries them (`SemanticFunction`'s definition,
call targets and error types; `FileItems`). A saved memo restored in another process would hold
numbers that mean other items there, and a partial reuse (some functions reused, the rest
recomputed) would compare ids from two different tables. Persisting the facts first needs
identity that survives a process: the same definition gets the same id whichever process
interns it.

## Status of the work

1. Vendor the patched crates; build and test cordial against them. **Done.**
2. Make item identity stable across processes.
3. Save the database when a run ends and restore it when the next begins, keyed by the cordial
   binary that wrote it; any failure to restore starts cold.
4. Persist cordial's queries, with stable keys.
5. Measure on a multi-crate workspace.
