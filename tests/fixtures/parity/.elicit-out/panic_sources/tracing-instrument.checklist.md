# Tracing instrument checklist

_Generated: 2026-08-09_

**Open gaps:** 5

Add `#[instrument]` (or `#[tracing::instrument]`) to each item below.

## `panic_sources`

### crate root

- [ ] `explodes` — `src/lib.rs:1` (pub)
- [ ] `not_possible` — `src/lib.rs:5` (pub)
- [ ] `expects` — `src/lib.rs:9` (pub)
- [ ] `unwraps` — `src/lib.rs:13` (pub)
- [ ] `compile_fail` — `src/lib.rs:17` (pub)

