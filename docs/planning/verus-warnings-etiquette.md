# `verus_warnings` etiquette

Verus is a rustc fork. It emits diagnostics `cargo check` / `clippy -D
warnings` never see, and its CLI has no deny-warnings switch. Catch them
in post: compile the crate with `verus`, and treat every compiler
`warning:` as an action item.

Amenable canary (2026-08-23, Verus 0.2026.05.22): two open sites —
unused `impl_tuple_evidence`, and `#[derive(Clone)]` that Verus cannot
specify yet.

Complements [allows](../../src/etiquettes/allows/mod.rs) (rustc `#[allow]`
inventory, including Verus `reason =` on `vstd` imports). This etiquette
is about the **other** compiler's output, not attributes in source.

---

## What counts

| Shape | Flagged |
| --- | --- |
| `warning: …` plus a `--> file:line:col` span | yes — one finding per unique `(file, line, message)` |
| `warning: N warning(s) emitted` | no — rustc/Verus summary |
| `error:` / verification failure | no — this lint is warnings-only |
| crate with no Verus surface | no — `verus` is not invoked |
| `verus` missing from `PATH` | no — skip the crate (quality must still run) |

A crate is a Verus target when its package name ends in `_verus`, or its
manifest lists `vstd`, `verus_builtin`, or `verus_builtin_macros`.

Invocation (matches amenable / elicitation):

```text
verus --crate-type=lib src/lib.rs
```

from the crate root. `src/main.rs` uses `--crate-type=bin`. Override the
binary with `CORDIAL_VERUS`, `VERUS`, or `VERUS_PATH`.

---

## Design

Parse rustc-style diagnostics from combined stdout/stderr. Dedup because
Verus reprints some warnings after verification. Hooks: source loader,
scope + inventory + attribute enrichers, probe, assessor, CSV / checklist
/ summary. Feature `verus_warnings`, in `quality`.

| Task | Detail |
| --- | --- |
| Scan + etiquette bundle | done |
| Tests in `tests/verus_warnings_etiquette.rs` | done |
