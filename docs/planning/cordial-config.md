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

```toml
[visibility]
max_crate_names_for_flat = 50
min_module_names = 10
prefer_root = true

[modularity]
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
min_distinct_kinds = 2
min_occurrences = 5

[tracing]
# extra_skip = ["inventory"]
```

Add a table per etiquette as new knobs appear. Tracing role→level maps stay
in code until dogfood needs a project override.

## Status

Implemented (`tests/cordial_config.rs`). Visibility, modularity (including
types-per-file, module-size 2σ, lower-tail ignore, hotspot diagnosis, and
hierarchy lints: top-heavy, lopsided, unary-nest collapse), cfg_scatter, and
tracing (`extra_skip`) read through `load_session_config`.
Role→level maps stay in code.
