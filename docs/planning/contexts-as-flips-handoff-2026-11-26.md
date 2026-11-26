# Contexts as flips: handoff, 2026-11-26

Supersedes the status sections of `contexts-as-flips-handoff.md` (2026-11-25). Read `contexts-as-flips.md` for the plan and
`CLAUDE.md` for the rules. This file is what a new agent needs to pick up the unfinished work without repeating this session.

## Update (same day, later): sections 1, 2 and the blocked default switch are resolved

Commit `7e44ed70` committed the whole working tree and **made `Flips` the default**. The restored cross-crate test is green. The causes, in the order they were found and
fixed (each by the method in section 2): keyed floor (seed from the persisted cells at restore, `workspace.rs`); a whole-module `origin` read (`DefMap::module_origin`); and
`include_macro_invoc` being a non-persisted query over every module (now `persist`). The single-crate restore test had asserted `unless_kani` was unreached; it is gated by
`cfg(not(kani))` so it is legitimately evaluated again, and the assertion is only `ordinary` now. `verifying_a_workspace_with_everything_makes_no_functions_evidence_again` replaced the
"runs nothing" assertion (flips must make the file and crate lists it changed and put back once; what must not repeat is a function's evidence). Passing under the new default:
`hir_def` (513), `cfg_atoms` (43), `an_edit_between_runs_reaches_the_taken_up_contexts`, `a_restored_contexts_facts_name_the_ordinary_functions_they_call`, the edit-reuse cfg-context test,
the save-verification test, both restore tests, the in-process tests. **Not re-run since the default switch:** the rest of the suite that uses contexts (about 80 tests; 87 of 107 passed under flips earlier
in the day, the 20 failures are all deleted, restated or fixed). If one fails, apply the "what does a failure mean" rule before touching it.

**Next: Phase 3 deletion (section 3, step 3)**, stages (a) to (e), named tests only, then the doc correction (step 4) and one amenable run (step 5). The `cfg_context_mode` key is to be **removed**
(`copies` becomes an error). Everything in sections 1 and 2 below describes how the work got here; read it for the method and the traps, not as open work.

## 0. The goal, and where it stands

**Goal:** analyse cfg contexts by flipping the one base crate's cfg (`CfgContextMode::Flips`), correctly and without repeating
work, so that the copy design can be deleted (Phase 3) and cordial can run on itself (peak RSS must fit the machine).

**Done and committed this session** (all with tests, clippy clean):

| Commit | What |
| --- | --- |
| `207d850e`, `5c327294`, `eb0682d4` | An attribute path in code with no scope in the current cfg is "unresolved", not "not `instrument`". A cfg context that has the scope resolves it; the tracing inventory uses that. Fixes the 21 false `TRACING-MISSING-INSTRUMENT` findings flips gave in amenable (functions in `#[cfg(feature = "verus")]` modules). |
| `92d4d073` | `settle_ordinary` panicked ("cannot delete read-locked id") because a log line read every block before the bodies were made again. Removed. One 38 s test covers eight shapes. |
| `e6ad0255` | `function_declaration` and the impl/trait/enum ones are tracked queries returning a `bool`, so a name added to a module does not re-run what asked about one function. |
| `6ea69388` | Those four queries are `persist`ed (a restored process needs an old value to backdate against). |
| `d1348e38` | `SourceToDefCtx::fn_to_def` finds a module-level function from where it is written (`hir_def::declaration::declared_function`), instead of building the map of everything the module declares. |
| `33d89237`, `bc6dcc0e`, `744228df` | Test triage: deleted tests that asserted copy-design counters, internal events, or totals. See section 4. |

Effect: **in one process, a function that no flipped atom reaches is now evaluated once** (it was 3 times). Tests:
`a_function_no_flipped_atom_reaches_is_evaluated_once_by_a_run_that_flips_and_restores` and
`a_function_that_makes_a_call_is_evaluated_once_by_a_run_that_flips_a_gate_in_its_module` (both `tests/saved_contexts.rs`).

**Not done:** a *restored* process still re-evaluates a function that calls into a crate whose map was re-made.
`a_restored_run_does_not_evaluate_again_what_no_context_reaches_in_either_of_two_dependent_crates` is **red** (it asserts `ordinary`
and `helper` are not evaluated again). That is the open problem (section 2). The default is still `Copies` (see section 3).

## 1. Uncommitted work in the tree (nothing here has had a clippy run since the last edits)

`git status`: `src/loader/database/options.rs`, `src/loader/database/session/workspace.rs`, and
`vendor/{ra_ap_hir,ra_ap_hir_def,salsa}`. Four independent things:

1. **`options.rs`: `#[default]` moved from `Copies` to `Flips`.** Do not commit until the kept tests (section 4) pass under it. With it, the
   restored cross-crate test is red.
2. **Seed the previous map from the persisted cells at restore** (the fix for the keyed floor, section 2).
   - `vendor/salsa`: `Ingredient::live_struct_ids`, `Zalsa::live_struct_ids`, `salsa::plumbing::live_struct_ids(db, name)`.
   - `hir_def/nameres/previous.rs`: `Previous` has a second field (`Seeds`), `seed_from_cells`, `take_seed`; `forget_all` (test only).
   - `hir_def/nameres.rs`: `pub fn seed_previous_maps_from_cells(db: &mut dyn SourceDatabase)` (seeds, then `synthetic_write` to begin a
     revision), and `crate_local_def_map` uses `recall(..).or(seeded)` as the "before" for `touch_changed_names`.
   - `session/workspace.rs`: calls it right after the saved database is deserialized, before `load_workspace_into_db_with_vfs` writes anything.
   - Unit test: `resolving_a_path_into_another_crate_does_not_run_again_when_a_name_is_added_to_a_root_after_a_restore` (`hir_def`, 10 ms).
     It was red, is green, and is the best demonstration of the mechanism.
3. **`module_origin`**: `ModuleOrigin` gained `definition_source_file_id`, `definition_source_range`, `declaration_source`,
   `declaration_source_range` (`ModuleData` delegates); `DefMap::module_origin(db, id)` reads only the cell's `origin` field. Call sites switched in
   `attrs.rs:420`, `has_source.rs`, `semantics.rs`, `diagnostics.rs`. Unit test: `asking_where_a_module_comes_from_does_not_run_again_when_a_name_is_added_to_it`.
   `hir_def` 512/512 and `cfg_atoms` 43/43 pass with 2 and 3. Not touched: `symbols.rs:542`, `decl_check.rs:175`, macro expansion tests.
4. **Instrumentation** (keep): target `cordial::whole_module` at DEBUG. `try_module_data` and `DefMap::modules` log, for workspace crates only, the module
   or crate, the call site (`#[track_caller]`, also on `Module::children`, `Module::scope`, `ChildBySource`) and the top query frames.
   `cordial::def_map` INFO says how many crates and modules the restore seed took.

Suggested: finish the diagnosis below, run `cargo clippy --tests -- -D warnings` once, then commit whole files (one commit is fine;
`memory/feedback_fast_commits.md`).

## 2. The open problem and the evidence

**Symptom (restored process):** `ordinary`, a function that calls a function of another crate and reads no flipped atom, is evaluated again.
`salsa::stale` says its `direct_facts` memo is out of date because a **module cell's `data` field** changed (keys 82608/82609).

**Mechanisms verified in the code:**

- **Flattening** (`vendor/salsa/src/flatten.rs` header): before saving, every non-persisted query a persisted memo depends on is replaced by the
  persisted inputs it read. A memo can only be protected from a change by a *persisted* query in between whose answer is unchanged (backdating needs the
  old value). This is why `function_in_scope` had to be persisted.
- **Keyed floor** (`vendor/salsa/src/keyed.rs`, `hir_def/nameres.rs::touch_changed_names`): with no previous map, `touch_all` raises the crate's floor and every
  name read of that crate is stale. A restored process has no `Previous`. That was the first cause; item 2 above fixes it, and the `keyed` edge no longer appears in
  `salsa::stale`.
- **Tracked structs must be re-made before they are read in a revision.** Reading old cells inside the map query broke the remake (the new function was missing
  from the map). Hence the seed is taken at restore, then a revision is begun. Do not move the seed into a query.
- A `ChildBySource` for a module reads the whole scope; `fn_to_def` no longer uses it for module-level functions (`d1348e38`).

**Current lead (not yet confirmed):** the remaining `data` edge is flattened from a non-persisted query that **enumerates all modules of the user's crate** via
`DefMap::modules` (which reads every cell's `data`). The last diagnostic listed the enumerators of workspace crates reached in the test:
`trait_impls_for_crate` (hir_ty `method_resolution.rs:753`, persisted, so a firewall already), **`include_macro_invoc`** (`hir_def/src/lib.rs:1632`),
`crate_functions`/`crate_files` (cordial enumerations, not on the facts path). `include_macro_invoc` is a direct dependency of `ordinary`'s `direct_facts`
(it is in the `read_in_order` list). **First thing to check:** is it persisted? If not, it is the likely culprit; the fix is the same pattern as
`function_in_scope` (a persisted query returning a small answer, or a keyed read), proved first by a `hir_def` unit test (wrapper query +
`db.log_executed`, see `nameres/tests/incremental.rs`, the `asks_*` helpers). Other non-persisted candidates on `ordinary`'s dependency list:
`fn_sig_for_fn`, `InferenceResult::for_body_`, `Body::with_source_map_`, `trait_environment_query`, `TraitSignature::of_`, `ImplSignature::of_`,
`impl_trait_with_diagnostics`, `GenericPredicates::query_with_diagnostics_`, `TraitImpls::for_crate_and_deps_`. Verify with
`#[salsa::tracked(persist)]` grep before assuming.

**Method that worked (use it, do not guess):** (1) minimal red test; (2) name the reader with `cordial::whole_module` / `cordial::semantic::reach`
(`direct_facts` events carry the function name, crate and `read_in_order`) / `salsa::stale`; (3) reproduce the principle in a `hir_def` unit test (ms);
(4) fix; (5) confirm with the integration test. Two hypotheses were falsified this way (path resolution is keyed correctly; `lang_items` is already persisted).

**What cannot be fixed, and is not the goal:** a restored flips run cannot compute *nothing* for its contexts. A process holds one value per query, so what only the
flipped context has (`harness`) and the lists that differ (`file_items`) are evaluated once on re-entry. The two restore tests were restated accordingly (section 4).

## 3. Phase 3 plan (agreed order, with Erik)

1. Instrument-attribute fix, flips only. **Done.**
2. Make flips the default. **Blocked** on the kept restore/verify/edit tests passing, or on Erik deciding to accept the restored-run cost.
3. Remove the copy design in stages, named tests only: (a) `CfgContextMode::Copies`, the `cfg_context_mode` config key (**remove it; `copies` becomes an error**), the copy branches
   in `contexts/pass.rs` and `leaves.rs`, `copy_functions.rs`; (b) `context_shares.rs`, `shared_copies`, `SavedContext`; (c) `add_context_crates` and the copy half of `ContextCrates`,
   the `item_key.rs` copy-root mapping (`original_path`, `copy_root_of`); (d) vendor: `Crate.canonical`, the `shared_modules` query; (e) copy-only tests.
4. Correct `contexts-as-flips.md` Phase 3: `contexts/{flips,gates,reuse}.rs` are **used by flips** (`Flips`, `Reach`, `Classified`, `Leaves`, `moves_modules`, `flipped_names_of`) and stay; in
   `nameres/shared.rs` only the `shared_modules` query goes (`close_over`, `split`, `report` are used by map reuse). The parity gate was **waived** (copies is itself wrong on the tracing
   findings), and the oracle is the source. Record that.
5. One amenable flips run, findings judged against the source for the three groups below.

**Amenable findings, three groups** (copies run 5.32 GB / 2.49M queries; flips 4.67 GB / 2.19M; flips ran without a panic):
1. 21 extra `MISSING-INSTRUMENT` in flips: **fixed** (section 0).
2. 2 extra `PROOF-INSTRUMENT` in flips (`KaniVerifier::name`, `CanaryVerifier::name`, which carry `cfg_attr(not(kani), instrument)`): **open, a policy question** (`is_forbidden_instrument` flags any
   instrument on a proof-only function, even a gated one). Flips now recognises them as instrumented; the rule may be what is wrong. Not checked with a fresh run.
3. 2 findings copies has and flips lacks (`ChronoVerifier::name`, `JiffVerifier::name` as missing; the source has the gated instrument): **copies is wrong**; leave it.

## 4. Tests: what is kept and why

Rule Erik set: **keep a test only if a failure means a user-visible regression (wrong findings, too much memory, repeated work named by function). Delete counter, event and total
assertions. Budget one projection (about 20 s, the sysroot load) per test.** Deleted this session: the copy-design counter tests, three fallback-event tests, the `<40` and
"steps allowed" restore tests, three of four save-verification variants, the two `memory_pressure` metric tests, the eight-shape duplicate. Kept:

| Test | Time | State |
| --- | --- | --- |
| `hir_def` (whole crate), `cfg_atoms` | 1.3 s, 0.6 s | green (512, 43): the soundness oracle, run it after any `hir_def` change |
| `tests/instrument_identity.rs` (2) | 27 s | green |
| `tests/flipped_instrument_evidence.rs` | 22 s | green (feature-only module, alias, look-alike, guard) |
| `tests/saved_contexts.rs`: the in-process call test, the in-process no-atom test, `a_restored_run_does_not_evaluate_again_a_function_the_context_does_not_reach` | 35 to 60 s | green |
| `tests/saved_contexts.rs`: `..._in_either_of_two_dependent_crates` | 60 s | **red under Flips** (the open problem); green under the committed Copies default |
| `tests/verify_before_save.rs::verifying_a_workspace_with_everything_runs_nothing_again` | ? | **not run under Flips since the changes** |
| `tests/edit_reuse.rs::an_edit_to_a_body_computes_the_functions_of_its_file_when_the_crate_has_a_cfg_context` | ? | **not run under Flips since the changes** |
| `tests/context_flips.rs::no_shape_of_what_only_a_flipped_context_has_is_read_after_it_is_gone` | 38 s | green |

## 5. How to work (Erik's rules, learned the hard way this session)

- **Efficiency is not optional.** Before any build, test or run, know which decision it settles that reading code or existing logs cannot. One named test per command, never a battery, never "to see
  whether four pass". A cordial integration test costs at least 20 s; a `hir_def` unit test costs 10 ms, so prove ideas there first. Do not wait on a slow background run when you can do something useful.
- **No blame game.** On a red test, go straight to its cause with the tracing. No A/B runs, no swapping files to ask "was it already like this".
- **Prove, don't speculate.** A plan needs a minimal red repro first; a fix needs a unit test that is red for the stated reason. Erik rejected speculative plans repeatedly.
- **Commit fast:** one `cargo clippy --tests -- -D warnings` on the finished tree, then `git add` whole files and commit; never move tests in and out of files, never rebuild to commit. Never commit a red test.
  No `git stash`/`reset`/`checkout` swaps; only add/commit/status/diff/log/show.
- `cargo nextest run --test X name` (one `--test`, a name filter). Pipe through `rg` only to trim output; the harness truncates long output in the middle, and failure output includes the log, so print the assertion
  with `rg -A`. **The scratchpad directory vanished mid-session; do not redirect into it.** Never write cordial output to `/tmp`; the store is `~/.cordial/<project>/`.
- **Logs:** use `RUST_LOG` with bounded targets (`cordial::whole_module=debug`, `cordial::semantic::reach=debug`, `salsa::stale=debug`, `cordial::def_map=info`) and `--no-capture`. Never `RUST_LOG=debug` globally
  (it produced a 206 GB log once). Every id in a log carries its name.
- No `cargo fmt` (it reformats the whole workspace including vendor); no new `#[allow]`; tracing instead of prints; not a Python repo (no scripts in the tree).
- For an amenable run: `cordial clean -p amenable` is fine; the flips layer is `~/.cordial/cordial.toml` with `cfg_context_mode = "flips"` and **must be removed afterwards** (it is absent now).
  One run, filtered logs, say what it decides first.
- Memory notes (`~/.claude/projects/.../memory/`) are advisory, and Erik has said they do not change behaviour; do not add rule entries.

## 6. Next steps, in order

1. Check whether `include_macro_invoc` (`hir_def/src/lib.rs:1632`) and the other non-persisted enumerators above are persisted; write the `hir_def` unit test that shows the one that re-runs after
   `forget_all` + `seed_previous_maps_from_cells` + a flip; fix it (persisted firewall query or keyed read); re-run the red restored test (60 s).
2. Gate (`clippy --tests`), commit the uncommitted work (section 1), leaving `options.rs` out until step 3.
3. Run the two "not run under Flips" tests once each. If green and the restored cross-crate test is green, commit the default switch and begin Phase 3 deletion (section 3).
4. If a restored flips run still cannot reach the goal, tell Erik the measured cost (functions re-evaluated in the flipped crates) and let him decide, rather than tuning thresholds.
