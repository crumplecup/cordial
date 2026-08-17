# Panic sources checklist

_Generated: 2026-08-09_

**Open items:** 4

Inventory of `panic!`, `unreachable!`, `.expect(…)`, and `compile_error!` sites. Resolution strategies are out of scope for this checklist.

## `panic_sources`

### PANIC-SOURCE-COMPILE-ERROR

- [ ] `compile_fail` — `src/lib.rs:18` — `compile_error!("intentional fixture")`

### PANIC-SOURCE-EXPECT

- [ ] `expects` — `src/lib.rs:10` — `.expect("missing value")`

### PANIC-SOURCE-PANIC

- [ ] `explodes` — `src/lib.rs:2` — `panic!("boom")`

### PANIC-SOURCE-UNREACHABLE

- [ ] `not_possible` — `src/lib.rs:6` — `unreachable!("not possible")`

