# Error handling as plugin

Planning document for cordial's **error-handling plugin model**: how abort
sites (panics), error sites, chain preservation, internal compliance, foreign
types, and attenuation share one registration surface for any workspace.

Complements [coverage-as-plugin.md](coverage-as-plugin.md) and
[CORDIAL_PLAN.md](../../CORDIAL_PLAN.md).

---

## Priority correction: caller locations through `map_err`

Earlier guidance called `map_err(CrateError::from)` a preferred wrap even
when `From::from` (or a constructor it calls) records `Location::caller()`.
`Result::map_err` does not propagate `#[track_caller]`, so this records a
standard-library location. A bare `?` propagates the call site through
`Result::from_residual`; where `map_err` is needed, a closure such as
`.map_err(|err| CrateError::from(err))` records the closure's call site.

- [x] Correct the deep-dive examples, compiled explanations, generated
  attenuation advice, and planning text that recommend a function item for a
  caller-tracking conversion. Prefer bare `?` when the return error has the
  required `From` bridge; otherwise call the converter inside a closure.
- [x] Add a compliance finding for `map_err(Type::from)` and equivalent
  function-item converters when the cataloged constructor uses `#[track_caller]`.
  Keep ordinary converters without caller-location behavior valid. Give the
  finding a concrete bare-`?` or closure rewrite.
- [x] Add regression tests that execute all three forms and assert the stored
  file and line: bare `?` and a closure must identify the caller; passing the
  function item must demonstrate the bad location. Add compliance fixtures for
  flagged and allowed conversions, and verify generated advice does not
  reintroduce the wrong form.
- [x] Replace Cordial's own affected production call sites and run focused
  checks, then a zero-open build-tier `cordial submit` before committing.

`#[instrument(ret)]` also hid the call site by wrapping constructor bodies in a
closure. The source track-caller rule now flags that attribute on tracked error
constructors, and Cordial's constructors omit it.

---

## Problem

In `elicit_doc`, error analysis was a **family of complementary scanners**
(error sites, chain breaks, internal type graph, foreign types, attenuation)
registered and orchestrated separately. Cordial initially ported each as its
own quality `EtiquettePlugin`, duplicating enricher stacks and making
"run error handling on this workspace" a multi-plugin concern.

---

## Design goal

**Plugin** remains the root registration trait. **ErrorHandling** is a sibling
supertrait of **Coverage** — not a separate pipeline mode.

Every `ErrorHandling` plugin answers the same logical query:

1. **Scope** — which workspace crates are in the denominator?
2. **Layers** — which analysis passes run (sites → chain → internal → foreign → attenuation)?
3. **Policy** — how are gaps classified (attenuation taxonomy, compliance rules)?
4. **Artifacts** — CSV, checklist, summary sections (via constituent etiquettes)

Antipatterns stays a separate quality plugin; untyped error carriers
(`Box<dyn Error>`, `Result<_, String>`) overlap in the quality report rollup.
Panicking APIs (`panic!`, `unwrap`, `expect`, `unreachable!`) are an
**error-handling layer**, not a standalone quality plugin: library code is held
to internal error types, binary and test code to miette. Test `.expect` /
`.unwrap` (including `#[cfg(test)]` modules under `src/`) are checklist action
items, not CSV-only inventory. Library abort sites wrap the associated
error in the crate's internal type (`From` / `map_err` / `?`, preserving
`source()`) — including `fmt::Error` from `write!`/`writeln!` and
`proc_macro2::LexError` from `parse::<TokenStream>()`. There is no
inventory-only exemption for Result-returning abort sites. A match arm that
must not execute is `CordialErrorKind::Unreachable` (`CordialError::unreachable`),
not `unreachable!` / `panic!`: the source copies `file`/`line` from
`Location::caller()` so callers keep control through `?`. Prefer making every
arm of a sum type meaningful; `Unreachable` is the fallback when the type still
requires the arm.

Sample programs used by tests (a `panic!` inside a raw string that is later
written to a tempfile) are still abort sites in the scanning crate. The panics
scanner parses string literals that are themselves Rust and contain abort APIs.
Keep those samples under `tests/fixtures/` or `tests/parity/` — the same path
skip every quality scanner already uses — and `include_str!` them from the
test. Assertion messages such as `"panic!"` do not parse as a file and are
not flagged.

---

## Layer model

```text
Session
  └── registers Plugin(s)
        │
        ├── Quality plugins (Plugin only)
        │     tracing, antipatterns, …
        │
        ├── ErrorHandling plugins
        │     StandardErrorHandling  (any workspace)
        │       panics (library → internal errors; binary/tests → miette)
        │       sites → chain → internal → foreign → attenuation
        │
        └── Coverage plugins
              ElicitationCoverage, HomecomingStdCoverage, …
```

---

## Trait sketches (implemented)

### `ErrorHandling`

```rust
pub trait ErrorHandling: Plugin {
    fn scope_provider(&self) -> &dyn ErrorScopeProvider;
    fn policy(&self) -> &dyn ErrorHandlingPolicy;
}
```

### Supporting traits

| Trait | Role |
| --- | --- |
| **`ErrorScopeProvider`** | Workspace crate roster (default: all members) |
| **`ErrorHandlingPolicy`** | Which layers run; future: profile-specific classification |
| **`ErrorHandlingLayers`** | panics / sites / chain / internal / foreign_types / attenuation |

### `StandardErrorHandling`

| Aspect | Choice |
| --- | --- |
| Scope | All workspace members (`WorkspaceMembersErrorScopeProvider`) |
| Layers | Full stack when features enabled |
| Etiquettes | `panics`, `error_sites`, `error_chain`, `internal_error_chain`, `foreign_error_types`, `foreign_error_attenuation` |
| Plugin id | `error-handling` |
| Category | `PluginCategory::ErrorHandling` |

---

## Shared error IR enrichers

All error-handling etiquettes register the same stack:

```text
scope → error-ir-scan → error-flow → foreign-error-attenuation-inventory → attribute
```

`error-ir-scan` parses each source file once, then runs site, chain, and
compliance collectors in a **single AST walk** (`src/etiquettes/error_ir/visitor.rs`).
Type-graph facts for types that implement `Error` (or `#[derive(Error)]`)
across `src/**` use a focused item-only pass on the same parse. Native
sources may live next to their call site; `src/error.rs` is not required.
Named `source` **or `err`** fields count as typed bridges. Expression-level chain probes merge onto existing
error-site nodes when they share `(file, line)` — including every site
kind on that line, so a same-line `map_err`+`?` pair both see preservation.
Constructors that keep the foreign error (`From`, `syn_parse`, `json_parse`,
`cargo_metadata`, `Fmt`/`TokenStreamParse` via `From`, forwarding `err` plus
caller context) preserve the source chain. Use bare `?` for a `From` bridge,
or call a constructor inside a closure. A function item such as
`map_err(CrateError::cargo_metadata)` loses the caller location when the
constructor uses `#[track_caller]`. Tail-position `map_err(ctor)` without `?`
still preserves the source chain but has the same caller-location defect.
`chain_break` is that conversion only
when the chain layer did not mark it preserved (`invariant` stringify remains
a break). `?` on a foreign type that already has a type-graph `From` bridge is
preserved, not pending infrastructure.
`?` on the same expression as `map_err` is not a second chain break.
Attenuation advice names a crate error newtype (or a type-graph `From` bridge)
instead of elicit_doc `ErrorKind` / `*Source` templates.
`format!("{err}")` interpolation is stringify, same as `.to_string()` on the
error binding. Quality-report open counts unique compliance sites that are not
already chain-break/pending rows, and include `Result<_, String>` next to
`Box<dyn Error>`.

### Error architecture

Mechanical, recursive shape. The catalog is every type that implements
`Error` (or `#[derive(Error)]`) under `src/`, not the `src/error` path.
Those types are the list to connect:

```text
ParentError { kind: Box<ErrorKind> }
  └── ErrorKind
        ├── Io(IoSource)            // native source wrapping a foreign error
        └── Parse(ParseSource)      // native source boxing a nested Kind
              └── ParseKind
                    └── Syn(SynSource)
```

1. **Parent** — an `Error`-implementing struct whose `kind` field is
   `Box<*Kind>`. The umbrella is not itself the public error enum. Extra
   context belongs on the parent; that is why the Kind is boxed.
2. **Kind variants** — each variant is a 1-tuple of a **native source**
   that implements `Error`. Not a foreign error, not `String`, not another
   Kind. Kind enums themselves need not implement `Error`; they are pulled
   in because an error type boxes them (or their payloads implement `Error`).
3. **Native source (foreign origin)** — `source: ForeignError` plus owned
   `file: String` and `line: u32`. Write a custom `#[track_caller] fn new` that
   takes the error (and any extra context), then copies file/line from
   `Location::caller()` in that body. Do not store `&'static Location`. Do not
   pass `file`/`line`/`location` as arguments. `From::from`, if present, is
   `#[track_caller]` and delegates to `new`; it need not call
   `Location::caller()` itself.
4. **Native source (nested)** — `kind: Box<NestedKind>` plus owned `file`+`line`,
   same `new` shape. Nested Kind variants follow the same rules. Parent `From`
   and inherent constructors that return the parent must be `#[track_caller]` so
   they do not hide the call site from source `new`.
5. **Coverage** — every native source (every `Error`-implementing struct
   that is not the parent) appears as a Kind variant (no orphans).

| Rule | When |
| --- | --- |
| `ERROR-CHAIN-COMPLIANCE-ARCH-PARENT-001` | No parent boxing a Kind, error enum used as the parent, extra parent, or boxed type is not `*Kind` |
| `ERROR-CHAIN-COMPLIANCE-ARCH-KIND-BOX-001` | `kind` field is not `Box<_>` |
| `ERROR-CHAIN-COMPLIANCE-ARCH-KIND-VARIANT-001` | Kind (or leftover `*Error` enum) variant is not a native source that implements `Error` |
| `ERROR-CHAIN-COMPLIANCE-ARCH-KIND-FANOUT-001` | Distinct literal call sites exceed `[internal_error_chain] kind_fanout_max_call_sites` (default 3; zero flags the first) for a Kind variant or native-source constructor; review whether the failures need distinct typed variants |
| `ERROR-CHAIN-COMPLIANCE-ARCH-ORPHAN-SOURCE-001` | Native source is not a Kind variant |
| `ERROR-CHAIN-COMPLIANCE-SOURCE-SHAPE-001` | Native source missing `source` / owned `file`+`line`, or storing `&'static Location` instead of copying those fields |
| `ERROR-CHAIN-COMPLIANCE-SOURCE-TRACK-CALLER-001` | Native source missing `#[track_caller] fn new` that calls `Location::caller()` (not a helper, not `From` alone); `new` takes file/line/location as args; a parent/`From` wrapper lacks `#[track_caller]`; or a tracked constructor uses `#[instrument(ret)]` |
| `ERROR-CHAIN-COMPLIANCE-MAP-ERR-TRACK-CALLER-001` | `map_err` receives a caller-tracking constructor as a function item, losing the call site |

The architecture above describes the default `field_strategy = "owned"`.
`[internal_error_chain] field_strategy = "borrowed"` selects a second storage
policy for error types only: parent and nested `kind` fields use `&Kind`, Kind
payloads and every other non-`Copy` error field use references, and call-site
`file` uses `&'static str`. `Copy` fields such as `line: u32` stay owned. The
`ERROR-CHAIN-COMPLIANCE-FIELD-STRATEGY-001` rule checks those fields; the Kind
storage and source-shape findings follow the selected policy. General
`[antipatterns.static_refs]` advice continues to apply outside error types.

Call-site chain preservation (stringify / discard) remains a separate layer.
`std::error::Error::source()` forwarding is still inventory on the type graph.

Implementation: `src/etiquettes/internal_error_chain/architecture.rs`,
crate-level over library `src/` (modules reachable from `lib.rs`) so
parent/Kind/source may live in different modules. Binary-only files are not
in the parent/Kind catalog. CLI layout (`act` on clap types, thin `main`)
is a separate etiquette: `docs/planning/one-crate-cli-layout.md`.

(feature-gated; session dedupes when multiple etiquettes are active)

Implementation: `src/enricher/error/` and `src/etiquettes/error_ir/`.

### Mod-gated feature layers (not per-item cfg)

Merging three formerly-independent scanners into one visitor risked scattering
`#[cfg(feature = ...)]` across dozens of struct fields, consts, and match arms
inside a single file — 82 occurrences in `visitor.rs` at one point. Instead,
`error_chain`- and `internal_error_chain`-specific logic each live in their own
file (`error_ir/chain_layer/`, `error_ir/compliance_layer.rs`), gated as a
whole unit by **one** `#[cfg(feature = ...)]` on the `mod` declaration in
`error_ir/mod.rs`. Nothing inside either layer file needs its own `#[cfg]`.

The core `visitor.rs` holds a `ChainLayer`/`ComplianceLayer` field (cfg'd once)
and calls into layer hook methods (`on_expr_try`, `on_item_impl`, `on_map_err`,
…) through a shared, feature-independent `SiteCtx` (module/fn context + file
paths — plain data, no gated types). This dropped `visitor.rs` from 82 scattered
cfg attributes to 27 concentrated at real boundaries (struct fields, delegation
call sites), and the two layer files to zero internal cfg. `ErrorIrScanLayers`
also lost ~20 cfg attributes on plain `bool` fields/consts that never needed
gating — only fields holding feature-gated *types* (`Vec<ErrorChainRecord>`,
etc.) require `#[cfg]`; flags and control data don't.

Same pass also fixed several latent narrow-feature-combination bugs this
surfaced (`enricher/error/mod.rs`, `enricher/error_flow.rs`, `ir/workspace.rs`,
`plugins/mod.rs` had imports that only compiled under wider bundles like
`quality`/`full`, never in isolation — `ForeignErrorRecordKind` moved from
`foreign_error_types` down into `error_sites`, since the always-on
`ErrorFlowEnricher` needs it regardless of that etiquette's feature).

This module-per-layer pattern is a reasonable template for other places in
the crate with high `cfg(feature)` density (411 occurrences workspace-wide as
of this writing) — apply it when a single file/struct legitimately needs to
merge cross-cutting, feature-optional logic; prefer plain mod-level gating
(already used in `etiquettes/mod.rs`, `enricher/mod.rs`) everywhere else.

---

| Task | Detail |
| --- | --- |
| `ErrorHandling` supertrait + scope/policy types | done |
| `StandardErrorHandling` unified plugin | done |
| Remove per-etiquette error plugins from `quality_plugins()` | done |
| Session + targets treat `ErrorHandling` like quality for workspace members | done |
| Shared `ERROR_IR_ENRICHERS` stack (explicit registration, no session injection) | done |
| Unified syn scan (`ErrorIrScanEnricher`, one parse per file) | done |
| Unified AST walk (`error_ir` visitor, one traversal for sites/chain/compliance) | done |
| Mod-gated `chain_layer`/`compliance_layer` split (cfg on mod, not per-item) | done |
| Move panics etiquette onto `StandardErrorHandling` | done |
| Surface split: library → internal error types, binary/tests → miette | done |
| SNR: `src/error.rs` type graph, preserve forwarding wrappers, panic checklist vs inventory | done |
| Bare `?` or a `map_err` closure is the preferred typed wrap; chain break = un-preserved converter only | done |
| `Result<_, String>` antipattern (newtype error carrier, rolls into error-handling) | done |
| SNR: format! stringify, QuestionMark≠chain_break, type-graph advice, unique rollup | done |
| Dogfood: typed `CargoMetadata`/`json_parse`/`CliError`; `map_err(ctor)` preserve | done |
| SNR: test `.unwrap` / `.expect` are checklist (miette); `#[cfg(test)]` in `src/` is the test surface | done |
| Library Result-returning abort sites → `CordialError` (`From` wrap of associated error, including `fmt::Error` / `LexError`) | done |
| Binary: `cordial_cli` surfaces failures with miette (`src/boundary.rs`, linked from `main` only) | superseded — one crate, thin `main`, no `BinaryError` |
| Tests: harness abort sites → `miette::Result` + `into_diagnostic` / `ok_or_else` | done |
| Embedded fixture programs in string literals are scanned; samples live under `tests/fixtures/` | done |
| `IrView::root`, `IrMut::insert_node`, `rebuild_path_index` return `CordialResult` | done |
| Dogfood panics: checklist **0**; tests **0** miette items; no inventory-only abort sites; abort-site action items **0** | done |
| Source-wrapper shape (`source` + owned `file`/`line` copied from `Location::caller()`; do not store `Location`) | done |
| Dead match arms → `CordialErrorKind::Unreachable` (location on the source; no `unreachable!`) | done |
| Error architecture suite (`Error` impls under `src/`; parent boxes Kind; native sources; recurse) | done |
| Dogfood: `CordialError` / `CliError` follow parent + boxed Kind + native sources; bin miette wrappers excluded | superseded — `CliError` folded into `CordialError` |
| One crate: CLI native sources on `CordialErrorKind`; `Cli::act`; miette only in `main` | done |
| Profile-specific `ErrorHandlingPolicy` layer gating in session | future |

---

## References

- [coverage-as-plugin.md](coverage-as-plugin.md)
- `src/plugin/error_handling.rs`, `src/plugins/error_handling.rs`
