# Tracing instrument checklist

_Generated: 2026-08-09_

**Open gaps:** 3

Add `#[instrument]` (or `#[tracing::instrument]`) to each item below.

## `mixed_visibilities`

### crate root

- [ ] `private_fn` — `src/lib.rs:1` (pub)
- [ ] `crate_fn` — `src/lib.rs:3` (pub(crate))
- [ ] `public_fn` — `src/lib.rs:5` (pub)

