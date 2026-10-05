# `amenable-ext-jiff` coverage etiquette

Config-driven multi-target registration (`[[amenable_ext.target]]`,
generic `ExtRowRule`/`build_ext_etiquette(target)`, owned `Arc<dyn
Etiquette>`) landed per
[amenable-ext targets in cordial.toml](amenable-ext-targets-config.md);
that doc is now the current reference for how targets are generalized and
registered. This doc stays the architecture reference for the
plugin/probe/assessor/reporter shape itself (still accurate — the shape
didn't change, only how many targets instantiate it and where the target
name comes from).

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
| Tests | `tests/amenable_ext_registry.rs` (pure-function unit tests, no real jiff build); real end-to-end run (`cordial coverage` against the `amenable` repo's real `amenable_ext` crate) exercised manually and passing — not yet wired into CI |
| `amenable_ext_jiff` skip-map / patch set | not yet created — empty until a real exception is found and reviewed |
| `just` recipe in the `amenable` repo | added: `just cordial-coverage` |

The first real end-to-end run surfaced two genuine `build_shadow_dep_rustdoc`
bugs, since `amenable_ext`'s dependency on `jiff` is optional (unlike every
prior shadow-dep consumer, which depends on its upstream unconditionally):

- `cargo rustdoc -p jiff` fails outright (`package ID specification 'jiff'
  did not match any packages`) when nothing in that invocation activates
  the optional dependency — cargo excludes an unactivated optional
  dependency from the resolved graph entirely, so it isn't addressable via
  a bare `-p` package spec. Fixed: `collect_member_dep_build_config`
  (`dep_features.rs`) now detects `dep.optional` and resolves the member
  crate's own activating feature (searching its `[features]` table for an
  entry naming `dep:{crate}`/`{crate}`/`{crate}/...`); when found,
  `build_shadow_dep_rustdoc` switches to a new `run_cargo_doc_for_optional_dep`
  (`cargo.rs`), which runs `cargo doc -p {shadow_crate} --features
  {activating_feature} -Z unstable-options --output-format json` (no
  `--no-deps`) and reads the upstream's JSON out of the resulting
  `target/doc/{upstream}.json` — `cargo doc` documents the whole resolved
  dependency graph by default, so the optional dependency's JSON is a
  side effect once its activating feature is on. Covered by
  `tests/shadow_dep.rs`'s new `optional_dep_resolves_its_activating_member_feature`
  / `non_optional_dep_has_no_activating_member_feature`, against a new
  minimal fixture workspace (`tests/parity/workspaces/optional-dep-workspace`).
- `std` and `amenable-ext-jiff` coverage share one cached registry dump
  (`registry_dump_path`), built by `cargo run -p amenable --features
  {AMENABLE_DUMP_REGISTRY_FEATURES} -- dump-registry` — the dump binary
  never linked in `amenable_ext` at all (its facade feature is `jiff`, not
  `creusot`/`verus`), so every `ExtStandard<T>` row showed as missing
  evidence regardless of what was actually registered. Fixed by adding
  `jiff` to the shared `AMENABLE_DUMP_REGISTRY_FEATURES` constant — a
  superset serving every active coverage plugin, not just whichever one
  happens to build the cache first; a future ext target adds its own
  activating feature name there too.

Verifying the fix end-to-end also surfaced two real gaps back in the
`amenable` repo itself (not cordial bugs): `amenable_ext`'s Verus witness
macro never called `inventory::submit!` for a `ProofRecord` (unlike its
Kani/Creusot siblings), and the facade's own `verus` feature never wired
`amenable_ext?/verus` at all (an orphan feature nobody could turn on). Both
fixed in `amenable`; the checklist now shows `jiff::Timestamp`/`Zoned`/
`civil::DateTime` as **Complete** (kani + creusot + verus + proof_test),
not partial.
