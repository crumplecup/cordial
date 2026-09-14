# `amenable-ext-jiff` coverage etiquette

The `amenable_ext` (third-party target crate) counterpart of
`amenable-std`: registry evidence + verifier witnesses over jiff's
registered carriers, via a shadow-dep rustdoc build instead of the shared
sysroot cache. Requested from the `amenable` repository's
`docs/AMENABLE_EXT_PLAN.md`, Phase 2.

Architecture: [Coverage as plugin](coverage-as-plugin.md). This etiquette
is a mechanical mirror of `amenable-std`'s own plugin/probe/assessor/
reporter shape (see that etiquette's own files for the template), not a
new pipeline.

---

## Why a shadow-dep build, not the sysroot cache

`amenable-std` inventories `std`/`core`/`alloc` from the shared sysroot
rustdoc cache — no Cargo dependency edge involved. A third-party target
crate (`jiff`) has no sysroot equivalent; its rustdoc has to come from a
real `cargo rustdoc` build of the crate itself.

`crate::cargo_rustdoc::build_shadow_dep_rustdoc(project_root, store,
shadow_crate, upstream_crate, force)` already does exactly this, and
already resolves feature activation via a plain `cargo_metadata` read of
`shadow_crate`'s own `Cargo.toml` dependency edge on `upstream_crate`
(`collect_member_dep_build_config`) before falling back to the
`elicitation`-specific tracked-shadow-pair lookup. Once `amenable_ext`'s
`Cargo.toml` declares a real (even optional) dependency on `jiff`, this
just works — no `elicitation`-style shadow-pair registration needed. The
"shadow" in these names is a legacy of `elicitation`'s own first use, not
a real constraint here.

`src/framework_std/ext_inventory.rs`'s `load_ext_inventory_from_shadow_dep`
is the one new inventory loader: it calls `build_shadow_dep_rustdoc`, then
reuses `load_std_inventory_from_json` (already crate-name-generic despite
living in `framework_std`) on the resulting cache path.

## Reused directly, not forked

Most of `amenable-std`'s machinery only needed generalizing, not forking:

- `StdInventoryItem`, `framework_std_type_items`, `RegistryDump`,
  `VerifierSkipMap`/`load_verifier_skip_map` (already keyed by an
  arbitrary `patch_set` string), `collect_proof_chain_subjects`,
  `run_amenable_dump_registry`/`load_registry_dump`,
  `ensure_registry_dump_for_assessor` — used as-is.
- `AmenableStdEntry`/`AmenableStdReport`/`AmenableStdStatus`/
  `ClassifyRowArgs` — reused as-is; nothing in their fields was
  std-specific, only the classify/build functions that populated them.
- `render_amenable_std_coverage_csv`/`render_amenable_std_gaps_csv` —
  reused as-is; their CSV headers were already generic text.

What genuinely needed a parallel implementation, because the *matching
logic* (not just the data shape) was hardcoded to `RustStdStandard<T>`:

- `registry.rs`: `parse_ext_standard_inner`/`evidence_for_ext_type`/
  `witness_verifiers_for_ext_type`/`ext_type_has_proof_test`, each a thin
  wrapper around a new private generic core
  (`parse_wrapped_standard_inner`, `evidence_for_wrapped_type`, …)
  parameterized by prefix family / lookup closure — the std versions
  became thin wrappers over the same generic core, not rewritten.
- `amenable.rs`: `classify_amenable_ext_row`/`build_amenable_ext_report`/
  `build_amenable_ext_gaps`/`amenable_ext_gap_fields`, same
  wrap-the-generic-core treatment (`classify_wrapped_row`,
  `build_wrapped_report`, `build_wrapped_gaps`, `wrapped_gap_fields`).
- `amenable_render.rs`: `render_amenable_ext_checklist_md`/
  `render_amenable_ext_summary_md`, parameterizing the profile title,
  wrapper-type name, and patch-set/checklist-filename text the std
  versions had hardcoded.
- `ext_run.rs` (new file): `AmenableExtOptions` (a genuinely separate
  type from `AmenableStdOptions` — a third cache-refresh axis, the
  shadow-dep rustdoc build, that the std flow doesn't have) and
  `assess_amenable_ext_coverage`.

Two prefixes for `ExtStandard<T>`, not one, for the same reason
`RustStdStandard<T>` needs a pair: the registry's own evidence/proof-
record names are fully qualified (`amenable_ext::ExtStandard<...>`), but
`collect_proof_chain_subjects` reads bare type names (`ExtStandard<...>`)
out of real `proof_chain_test.rs` source text — a real, test-caught gap
in the first pass (`tests/amenable_ext_registry.rs`'s
`build_amenable_ext_report_classifies_complete_partial_and_missing`
failed on `proof_test()` until `PROOF_CHAIN_EXT_STANDARD_PREFIX` was
added).

## Plugin / etiquette wiring

`plugins/amenable_ext.rs` — `AmenableExtCoverage` (id
`amenable-ext-coverage`), `AmenableExtTargetProvider` (a `CoverageTarget::
upstream_dep("jiff")` row plus one `workspace_member` row per workspace
crate, mirroring `AmenableStdTargetProvider`'s `std_inventory` + member
rows). `etiquettes/framework_ext/` — `probe.rs`/`assessor.rs`/
`reporter.rs`/`jiff.rs` (Finding/Rule/disposition round trip), mirroring
`etiquettes/framework_std/{probe,assessor,amenable_reporter,amenable}.rs`
exactly. The probe/assessor fire on `ir.crate_name() ==
AMENABLE_EXT_IMPL_CRATE` ("amenable_ext"), the same pattern
`AmenableStdScopeProbe` uses for `AMENABLE_IMPL_CRATE`.

`reporter/coverage_summary.rs` gained an `amenable_ext_section` module and
matching arms for both `plugin.id() == "amenable-ext-coverage"` and
`etiquette_id == "amenable-ext-jiff"`, mirroring `amenable_section`.

Feature `amenable_ext = ["amenable_std", "shadow"]` — requires
`amenable_std` (shares its registry-dump/skip-map/proof-harness machinery
directly) and `shadow` (`build_shadow_dep_rustdoc`). Folded into `full`.

## Naming, one target only so far

Everything here is jiff-specific by name (`AmenableExtJiffAssessor`,
`AMENABLE_EXT_JIFF_ETIQUETTE`, `amenable-ext-jiff.checklist.md`, …), not a
generic "any ext target" abstraction — matching `amenable-std`'s own
scope (etiquette code specific to one target; only the shared
`framework_std` machinery is generic). A second target (e.g. chrono) gets
its own sibling files (`etiquettes/framework_ext/chrono.rs`, a
`AmenableExtChronoAssessor`, …), not a rename of this one; `AMENABLE_EXT_
IMPL_CRATE`/`build_amenable_ext_report`/etc. in `framework_std` are
already generic enough to be reused as-is for that target too.

## Status

| Task | Detail |
| --- | --- |
| Registry generalization (`registry.rs`, `amenable.rs`, `amenable_render.rs`) | done |
| Shadow-dep inventory loader (`ext_inventory.rs`) | done |
| Orchestration (`ext_run.rs`) | done |
| Plugin + target provider (`plugins/amenable_ext.rs`) | done |
| Probe + assessor + reporter (`etiquettes/framework_ext/`) | done |
| Coverage summary wiring | done |
| Tests | `tests/amenable_ext_registry.rs` (pure-function unit tests, no real jiff build); real end-to-end run (shadow-dep build → probe → assessor → reporter against the `amenable` repo's real `amenable_ext` crate) not yet exercised in CI |
| `amenable_ext_jiff` skip-map / patch set | not yet created — empty until a real exception is found and reviewed |
| `just` recipe in the `amenable` repo | not yet added |
