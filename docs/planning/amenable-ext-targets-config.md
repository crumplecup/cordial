# Amenable ext coverage targets declared in `cordial.toml`

Status: **Proposed**

Supersedes the "second target gets its own sibling files" stance in
[amenable-ext-coverage-etiquette.md](amenable-ext-coverage-etiquette.md).

## Problem

Each `amenable-ext` coverage target (today only jiff) is a bespoke Rust
module plus entries in `probe.rs`, `assessor.rs`, `plugins/amenable_ext.rs`,
and `reporter/coverage_summary.rs`. Reading `framework_ext/jiff.rs`, the
jiff-specific data is three strings: the upstream crate name, the rule-id /
category, and the patch-set key. Everything else (the row finding, status to
disposition mapping, report assembly) is target-agnostic and already reuses
`AmenableStdEntry` / `AmenableStdReport` / `AmenableStdStatus`.

Chrono is the second real target, which is the right moment to generalize:
two data points, before a third copy.

## Decisions

### Targets are runtime data, not compile-time statics

The target list belongs to the project running cordial, and `cordial.toml`
is already layered at runtime (defaults, `{store_home}`, workspace). Baking
the TOML into the binary (`include_str!`, `build.rs` codegen) would freeze the
list inside cordial, defeat the layering, and need generated statics to
avoid one `String` allocation off any hot path. Rejected.

`StaticEtiquette` stays the right shape for built-ins. Config-derived data is
the exception.

### Config shape: array of tables

```toml
[amenable_ext]
# enabled = true

[[amenable_ext.target]]
name = "jiff"
# enabled = true

[[amenable_ext.target]]
name = "chrono"
```

An array of tables, not `targets = ["jiff", "chrono"]`, because each target
will soon need its own knobs (features for the shadow-dep rustdoc build,
skips, `enabled`).

- `wrapper_trait` is **not** configurable. `ExtStandard<T>` is fixed by
  `ExtType`'s design; a knob that cannot change anything is noise.
- No `shape` / `kind` field yet. If a target ever needs different matching
  logic, that is the signal to add one.
- Built-in defaults stay a plain `const KNOWN_TARGETS: &[&str]` (jiff), so a
  project with no config behaves as today. Config adds, or disables, entries.
  No `build.rs`, no macro.

### One generic implementation, owned target name

`AmenableExtJiffRule` / `AmenableExtJiffRowFinding` and the jiff-only
siblings become one named type each (`ExtRowRule`, `ExtRowFinding`, ...)
holding the target name as an owned `String`. Per-target Rust types add
nothing.

Names derived from the target must reproduce today's jiff names exactly,
because exception files, CI, and downstream tooling may reference them:

| Derived from `jiff` | Value |
| --- | --- |
| etiquette id | `amenable-ext-jiff` |
| rule id | `AMENABLE-EXT-JIFF-ROW` |
| category | `amenable-ext-jiff` |
| checklist artifact | `amenable-ext-jiff.checklist.md` |
| enable toggle | `[amenable-ext-jiff] enabled = false` (kept) |

### Registration: owned etiquettes (`Arc`)

`Plugin::etiquettes()` and `Session::register` currently require
`&'static dyn Etiquette`, and `session/resolve.rs` extracts
`&'static dyn Loader` / `Probe` / ... from those. That bound is right for
built-ins but wrong for etiquettes derived from config.

Rejected: `Box::leak` through a helper (the pattern
`plugins/error_handling.rs` already uses). It accumulates in a long-lived
process that reloads config, and a `OnceLock` cache makes the first config
win for the whole process, which breaks tests that run many sessions with
different configs.

Chosen: make ownership honest. Etiquettes are held as `Arc<dyn Etiquette>`;
built-in `StaticEtiquette` statics are wrapped at the seam (`&T: Etiquette`
blanket impl). The hooks extracted in `resolve.rs` borrow from the session's
list instead of being `'static`.

**Plugins stay `&'static dyn Plugin`.** A plugin is a product identity, i.e.
code. What varies with config is the etiquette set, so
`Plugin::etiquettes` takes the session and returns owned handles:

```rust
fn etiquettes(&self, session: &dyn SessionView) -> Vec<Arc<dyn Etiquette>>;
```

The ext plugin reads `[[amenable_ext.target]]` through
`load_session_config(session)` when asked. (Rejected: making `Plugin`
`Arc<dyn Plugin>` too, which roughly doubles the diff and forces the CLI to
load config before it can build its plugin list.)

`cordial explain` has no session today. It resolves the project root the same
way other commands do (`-p` / cwd) and loads that project's config, so
config-derived etiquettes are listed.

Measured footprint (grep for `dyn Etiquette`): ~17 files, ~60 sites --
`plugin.rs`, `plugins/*`, `session.rs`, `session/run.rs`,
`session/resolve.rs`, `etiquettes.rs`, `etiquette/explain.rs`,
`examples/custom_plugins/*`, `tests/custom_plugins.rs`, `tests/explain.rs`.
Mostly signature plumbing. It is a public API break, accepted because cordial
is 0.1.0; `amenable` and `homecoming` need follow-up edits wherever they
implement `Plugin`.

### `coverage_summary` loops instead of matching

Replace the per-id `"amenable-ext-jiff" =>` match arms (and the second
`amenable_ext_jiff_section` call) with a loop over the plugin's own
etiquettes, keyed by target name.

## Open items to verify before coding

- Whether coverage rule ids appear in `KNOWN_IDS` / the order-table
  constraints. Runtime-derived ids cannot be in a const table; if they are
  there today, decide how dynamic ids are ordered.
- `cordial explain` for per-target etiquettes: it must list them after
  config load, not from a static table.
- Where target `features` for the shadow-dep rustdoc build come from today.

## Sequencing

0. **Ownership change.** `Arc<dyn Etiquette>` through `Plugin`, `Session`,
   and `resolve.rs`; hooks borrowed rather than `'static`; built-ins wrapped
   at the seam. No behavior change; the full suite and `just cordial-gate`
   pass unchanged. Lands as its own commit(s), ahead of any ext work.
1. **Pure refactor.** Generic finding / report / disposition types over an
   owned target name; keep the static jiff etiquette. Acceptance: jiff output
   (findings, CSV, checklist, summary) is byte-identical to before.
   Landed as the generic `row`/`probe`/`assessor`/`reporter` types in
   `etiquettes/framework_ext` plus `build_ext_etiquette(target)`, with
   `tests/amenable_ext_jiff_metadata.rs` checking the built etiquette's
   id/name/explain/rule text against the pre-refactor hardcoded constants
   (it caught one real regression: the explain page's rule summary had
   lost `ExtRowRule`'s capitalization of `target`, now fixed by deriving
   it from the real `ExtRowRule` instead of re-deriving the string).
   That test is metadata-only, not full output — no fixture workspace
   with a real `amenable_core::ExtStandard<T>` impl exists yet to drive
   the probe/assessor/reporter end-to-end, so the full "byte-identical
   findings/CSV/checklist" acceptance bar is still open; folded into
   step 2, which needs a multi-target fixture anyway.
2. **Config and registration.** `[[amenable_ext.target]]`, `KNOWN_TARGETS`
   default, owned per-target etiquettes, `coverage_summary` loop, `cordial explain`. Chrono
   lands as two lines of config in the consumer repo, with no new Rust files.

## Status

- [ ] Verify open items
- [x] Step 0: `Arc<dyn Etiquette>` ownership change
- [x] Step 1: generic types; metadata-level regression test in place. Full
      byte-identical output verification needs a fixture workspace with a
      real `ExtStandard<T>` impl — deferred into step 2
- [ ] Step 2: config-driven targets and registration
- [ ] Update `amenable-ext-coverage-etiquette.md` to point here
