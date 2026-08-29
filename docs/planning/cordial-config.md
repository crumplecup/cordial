# `cordial.toml` config

Canonical home for etiquette thresholds. Loaded with the [`config`](https://docs.rs/config)
crate — do not hand-roll TOML/JSON readers for these knobs.

## Files

Later sources win:

1. [`CordialConfig::default`](../../src/config.rs) — graceful fallback
2. `{store_home}/cordial.toml` — `~/.cordial` unless `CORDIAL_HOME` / `--store-home`
3. `{workspace}/cordial.toml` — project root (highest priority)

Missing or unreadable files do not fail the run; the `Default` impl is the
backup plan. Drift between `Default` and a committed `cordial.toml` is
accepted.

Every etiquette table accepts `enabled` (default `true`). Set
`enabled = false` to skip that etiquette for the project — that is the
line `cordial explain` points at. Per-site suppressions stay on
`cordial exceptions add`.

```toml
[visibility]
# enabled = true
max_crate_names_for_flat = 50
min_module_names = 10
prefer_root = true

[modularity]
# enabled = true
file_inventory_min_lines = 500
function_inventory_min_lines = 150
function_hotspot_min_lines = 80
file_checklist_min_lines = 1000
function_checklist_min_lines = 200
max_types_per_file = 10
module_size_sigma = 2
module_size_ignore_lower_tail = false
min_module_lines = 0
top_heavy_min_percent = 50
lopsided_min_percent = 75
hierarchy_min_lines = 150

[cfg_scatter]
# enabled = true
min_distinct_kinds = 2
min_occurrences = 5

[crate_attrs]
# enabled = true
# Both default true. Per-member exceptions keep an FFI crate free to use
# unsafe without turning the lint off workspace-wide.
# forbid_unsafe = true
# missing_docs = true
# allow_unsafe = ["ffi"]
# allow_missing_docs = []

[doc_warnings]
# enabled = true
# document_private_items = false
# all_features = false
# skip_crates = []

[derives]
# enabled = true
max_constructor_args = 3
min_fluent_setters = 2

[tracing]
# enabled = true
# extra_skip = ["inventory"]
# apply_gate_crates = { amenable_kani = "kani" }
# apply_skip_crates = ["amenable_verus", "amenable_creusot"]

[tracing.subscriber]
# All default true.
# init_in_main = true
# init_in_tests = true
# helper_in_lib = true
# rust_log_fallback = true
# idempotent = true
```

`apply_gate_crates` wraps `--apply`'s `#[instrument]` as
`#[cfg_attr(not(<cfg>), tracing::instrument(...))]` for that crate and
anything that compiles under the same graph-wide flag (transitive Cargo
dependents, plus `#[path]` splice consumers). `apply_skip_crates` never
writes `#[instrument]` (Verus bare compiler / Creusot translator);
skip does not follow ordinary deps, but does follow a `#[path]` splice.

Etiquettes with no other knobs still have a table so they can be turned
off: `[panics] enabled = false`, `[pageantry] enabled = false`,
`[impl-coverage] enabled = false`, and so on. Custom plugins with no
table stay on.

Add a table per etiquette as new knobs appear. Tracing role→level maps stay
in code until dogfood needs a project override.

## Status

Implemented (`tests/cordial_config.rs`). Every etiquette table has
`enabled` (default true); `CordialConfig::etiquette_enabled` gates the
session. Visibility, modularity (including types-per-file, module-size
2σ, lower-tail ignore, hotspot diagnosis, and hierarchy lints: top-heavy,
lopsided, unary-nest collapse), cfg_scatter, derives
(`max_constructor_args`, `min_fluent_setters`), tracing (`extra_skip`,
`apply_gate_crates`, `apply_skip_crates`, nested `[tracing.subscriber]`),
crate_attrs (`forbid_unsafe`, `missing_docs`, `allow_unsafe`,
`allow_missing_docs`), and doc_warnings (`document_private_items`,
`all_features`, `skip_crates`) read through `load_session_config`.
Role→level maps stay in code.
