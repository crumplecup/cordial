# Claude Project Instructions

## Quick Reference

| Category | Key Rule | Section |
| --- | --- | --- |
| **Architecture** | Read [CORDIAL_PLAN.md](CORDIAL_PLAN.md) before structural changes | [Design](#design) |
| **Naming** | Bundles of hooks are **Etiquettes**, not `Standard` (amenable owns that) | [Design](#design) |
| **Interfaces** | Prefer traits at seams (`Finding`, `Marker`, `IrView`, …), not public concrete structs | [Design](#design) |
| **Planning** | Track active work in [PLANNING_INDEX.md](PLANNING_INDEX.md) | [Workflow](#workflow) |

---

## Design

`cordial` is a refinement of `elicit_doc`. The authoritative architecture
document is [CORDIAL_PLAN.md](CORDIAL_PLAN.md). Key decisions:

- **Etiquettes** — named plugin bundles (loaders, enrichers, probes, assessors, reporters).
- **Graph IR** — `petgraph::StableDiGraph` per crate; probes read, enrichers mutate.
- **Query layer** — Rust `Query` trait over the graph; not SurrealDB/FalkorDB as primary IR.
- **Trait-first** — `Finding`, `Marker`, `Rule`, and `Artifact` are traits so users can swap formats.

When implementing, match conventions from sibling repos (`homecoming`, `amenable`,
`elicit_doc`) unless this document or `CORDIAL_PLAN.md` says otherwise:

- `derive_more` for errors, `tracing` + `#[instrument]` on functions
- Tests in `tests/`, not `#[cfg(test)]` in source
- `lib.rs` only `mod` and `pub use`

---

## Workflow

1. **Plan** — update or add a planning doc; list it in `PLANNING_INDEX.md`.
2. **Implement** — minimal diff aligned with `CORDIAL_PLAN.md` phases.
3. **Verify** — `cargo check`, `cargo test`, `cargo clippy` before commit.
4. **Document** — update planning doc status when a phase completes.

Pre-commit: fix all warnings and errors introduced by the change.

---

## Related Repositories

| Repo | Relationship |
| --- | --- |
| `elicit_doc` | Predecessor; logic to migrate as etiquettes |
| `homecoming` | Trait-first + petgraph precedent |
| `amenable` | Avoid `Standard` name collision |
| `elicitation` | Optional future SurrealDB export path |
