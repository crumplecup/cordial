# A definition map that readers depend on by module, not as one value

**Goal.** Flipping one cfg atom of a crate in place runs again what reads what the atom changed, and nothing else.
Measured now (`tests/cfg_atoms.rs`, flipping one atom of one crate in place): item trees are exact (one runs), but the
definition map is built again whole and **every body runs again** (4 of 4 and 40 of 40), because a body depends on
the `crate_def_map` query's value, a single `DefMap`, and that value differs when any module in it does. Nothing
downstream can be found equal until it has run.

This is the fix for the whole class: not a smarter closure over what a copy might share, but readers that name what
they read. It serves both the copies we make for cfg contexts (what they share would be what is equal, found by
salsa) and toggling a crate in place.

## The mechanism (salsa, not ours)

Salsa records a dependency on a **field of a tracked struct** and backdates it by that field's equality: a tracked
struct made again by the same query with the same identity fields keeps its id, and a field of equal value keeps
its `changed_at`. So a reader of one module's data depends on that module's data only.

1. `ModuleCell`, a tracked struct (identity: the `ModuleId`; one tracked field: the `ModuleData`). Made by the def-map
   query, once per module, when the collector has finished. `DefMap.modules` holds `FxIndexMap<ModuleId, ModuleCell>`.
2. The parts of a `DefMap` that are not a module and change independently get a cell each: the prelude and
   `macro_use_prelude`; `derive_helpers_in_scope` and `macro_def_to_macro_id`; the diagnostics (they hold the options
   of the crate, so they differ under any flip; a body never reads them).
3. The frozen `DefMap` is then ids and cells: equal when the module set is, so `crate_def_map` backdates when no
   module was added or removed, and a reader re-runs only if a cell it read changed.
4. Reads take the database: `def_map.module_data(db, id)` replaces `def_map[id]` and `def_map.modules`. The collector
   builds on a plain `ModulesMap` as it does now; the map is frozen into cells at its end, and the plain field is
   private afterwards so a reader that bypasses a cell does not compile.

## Stages

* **M1. Module cells.** `ModuleCell`, freezing at the end of `crate_local_def_map` and `block_def_map`, `module_data`
  and every read site of `DefMap[..]`/`.modules` in hir_def, hir_ty, hir, ide_db and the tool. Driven by the
  fixture: after flipping an atom only the module the gate is in re-reads, and the other 39 bodies do not run.
* **M2. The rest of the map.** The three cells above; `DefDiagnostic` stops carrying the options and reads the
  crate's at display.
* **M3. Sharing and persistence.** `shared_modules`, `own_modules`, `take_shared_child` and the retirement keys read
  and write cells; a saved state holds the cells (tracked-struct persistence) and a restore reads them; the
  restored-run tests and `a_restored_run_executes_few_rust_analyzer_queries` stay green.
* **M4. Contexts as flips.** With M1 and M2 a copy of an unchanged module is equal to the original's by salsa and
  not by our closure; measure what the copies still repeat and what the closure rules (S1) can be dropped for.

## What drives it

`tests/cfg_atoms.rs::what_a_flip_runs_again_does_not_grow_with_the_files_it_does_not_reach` (flip one atom of a
4- and a 40-module crate; assert the same bodies run in both) is written first and fails until M1 is done. No stage
lands with a red test; M1 and M2 may land together if a body still reads the diagnostics before M2.

## Progress

*M1 and the diagnostics of M2 (2026-11-24).* `ModuleCell` and `DiagnosticsCell` are tracked structs
(`nameres.rs`); `DefMap::freeze(db)` makes them at the end of `crate_local_def_map` and `block_def_map`;
`DefMap::module_data(db, id)`, `try_module_data` and `modules(db)` read them (a map still being built answers from
its plain `ModulesMap`, so the collector is unchanged), and `Index<ModuleId> for DefMap` is gone: about 130 read
sites in hir_def, hir_ty, hir and the tests now name the database. Result on the fixture: flip one atom of a 4- and
a 40-module crate and **one body runs, the one the flip makes visible** (4 and 40 before).

Two things in salsa that this needed, both found by a failing restore test:

* A persisted memo's edge to a tracked **field** was dropped on the grounds that the query that made the struct
  covers it. It does not once that query's value can be equal while a field is not (the whole point here); the edge is
  kept (`tracked_field.rs`).
* A restored memo's shared edge set listed a field before the query that makes it, and a field read first says
  "unchanged" because its query has not run again yet. A set is now checked queries first (`flatten.rs`).

One test changed its premise, not its claim. `a_save_that_keeps_the_encoding_...` edited a function and expected
reuse of chunks inside an ingredient the edit reached; the edit no longer reaches the library's ingredients at all
(that is the result), so it now edits in a macro call, which an ingredient holding every crate's expansions has to
look into. The persistence event gained `planned_chunks`, `keeping` and `clean_memos`, which is how this was found.

**Still whole-crate readers.** The inherent and trait impl maps (`hir_ty::method_resolution`), `lang_items`,
`include_macro_invoc` and the import map iterate every module, so they depend on every cell and run again on any
change. They are the next readers to split (a cell of impls per module); until then a flip runs these and not the
bodies. Not yet done: the prelude, `macro_use_prelude`, derive helpers and `macro_def_to_macro_id` in cells of their
own (they change only with a gated macro or derive); M3 (the sharing rules on cells) and M4.
