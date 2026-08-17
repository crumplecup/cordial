# Tracing instrument checklist

**Open gaps:** 2

Add `#[instrument]` (or `#[tracing::instrument]`) to each item below.

## `fixture_crate`

### `apply_target`

- [ ] `apply_target::missing` — `src/lib.rs:1` (pub)
- [ ] `apply_target::traced` — `src/lib.rs:5` (pub)
