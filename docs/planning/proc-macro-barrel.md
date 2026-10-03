# Proc-macro crates, and keeping logic out of the barrel

Status: **Done**

## Problem

`PAGEANTRY-BARREL-001` says `lib.rs` / `mod.rs` hold only declarations and
re-exports. In a `proc-macro = true` crate, rustc requires every
`#[proc_macro]`, `#[proc_macro_derive]`, and `#[proc_macro_attribute]` function
to be declared at the crate root, so the rule fires on code that cannot move
(`amenable_derive`: 23 findings).

A one-line exemption by attribute silences the findings but also lets any amount
of logic live in those functions. The better shape is: know what kind of crate
this is, define what a barrel means for that kind, and then flag logic that
leaks into it.

## Phases

### 1. Crate kind in the IR -- done

- `CrateKind { Lib, Bin, ProcMacro }` in `src/ir/crate_kind.rs`.
- `CrateTarget` carries `kinds`, filled in `discover_crate_targets` from
  `cargo metadata`'s own target kinds. Cargo already normalizes
  `[lib] proc-macro = true`, so no manifest parsing.
- `SourceLoader` passes the kinds to `SourceLoadView`; `populate_ir` records
  them on the crate root node as `crate_kinds` (JSON array of strings).
- `IrView::crate_kinds()` / `IrView::is_proc_macro()` read it back.
- **Unknown is not "plain library".** A synthetic target (no `Cargo.toml`) or a
  snapshot cached before this change has no `crate_kinds`, and reads as an empty
  list. Consumers must treat empty as "do not know".
- `enricher/error_flow.rs` had its own hand-rolled `[lib]` TOML scan for the
  same fact (and missed the `proc_macro` spelling). It now reads the IR.

### 2. Barrel by crate kind -- dropped

The lint does not need the crate kind. rustc already rejects
`#[proc_macro*]` outside a proc-macro crate root, so honoring the attribute
can never hide a real mismatch. The IR fact stays for other consumers.

### 3. Shim check -- done

What the lint actually wants is that `lib.rs` / `mod.rs` stay a table of
contents: primary types and logic live in a named file, and anything rustc
pins to the root only delegates.

- `PAGEANTRY-BARREL-001` exempts proc-macro entry points from the "no
  functions in a barrel" rule.
- `PAGEANTRY-BARREL-SHIM-001` flags an entry point whose body (lines between
  its braces) exceeds `[pageantry] max_shim_lines` (default 8).
- Findings say what and where (`#[proc_macro_derive] fn derive_entry has a
  14-line body; shims may have 8 ...`); the rule id is the exemption handle.
- `[pageantry]` now carries `enabled`, one switch per rule (`trait_block`,
  `barrel`, `barrel_shim`), and `max_shim_lines`.

Not pursued: statement counts, callee counts, or control-flow heuristics. A
line limit is cheap, explainable, and tunable; add signals only if dogfood
shows it is gamed.

## Status

- [x] Phase 1: crate kind detected and recorded; `error_flow` consolidated
- [x] Phase 2: dropped (not needed by the lint)
- [x] Phase 3: shim check with configurable limit and per-rule switches
