# Error handling as plugin

Planning document for cordial's **error-handling plugin model**: how abort
sites (panics), error sites, chain preservation, internal compliance, foreign
types, and attenuation share one registration surface for any workspace.

Complements [coverage-as-plugin.md](coverage-as-plugin.md) and
[CORDIAL_PLAN.md](../../CORDIAL_PLAN.md).

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
inventory-only exemption for Result-returning abort sites.

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
caller context) are the **preferred `map_err` wrap**, including the 1-arg
function-pointer form `map_err(CrateError::cargo_metadata)`. Tail-position `map_err(ctor)` without `?`
is the same wrap. `chain_break` is that conversion only
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
3. **Native source (foreign origin)** — `source: ForeignError` plus `file`+`line`
   (or `location`). Write a custom `#[track_caller] fn new` that takes the
   error (and any extra context), then reads file/line from
   `Location::caller()` in that body. Do not pass `file`/`line`/`location` as
   arguments. `From::from`, if present, is `#[track_caller]` and delegates to
   `new`; it need not call `Location::caller()` itself.
4. **Native source (nested)** — `kind: Box<NestedKind>` plus location, same
   `new` shape. Nested Kind variants follow the same rules. Parent `From` and
   inherent constructors that return the parent must be `#[track_caller]` so
   they do not hide the call site from source `new`.
5. **Coverage** — every native source (every `Error`-implementing struct
   that is not the parent) appears as a Kind variant (no orphans).

| Rule | When |
| --- | --- |
| `ERROR-CHAIN-COMPLIANCE-ARCH-PARENT-001` | No parent boxing a Kind, error enum used as the parent, extra parent, or boxed type is not `*Kind` |
| `ERROR-CHAIN-COMPLIANCE-ARCH-KIND-BOX-001` | `kind` field is not `Box<_>` |
| `ERROR-CHAIN-COMPLIANCE-ARCH-KIND-VARIANT-001` | Kind (or leftover `*Error` enum) variant is not a native source that implements `Error` |
| `ERROR-CHAIN-COMPLIANCE-ARCH-ORPHAN-SOURCE-001` | Native source is not a Kind variant |
| `ERROR-CHAIN-COMPLIANCE-SOURCE-SHAPE-001` | Native source missing `source` / file+line / `location` |
| `ERROR-CHAIN-COMPLIANCE-SOURCE-TRACK-CALLER-001` | Native source missing `#[track_caller] fn new` that calls `Location::caller()` (not a helper, not `From` alone); `new` takes file/line/location as args; or a parent/`From` wrapper lacks `#[track_caller]` |

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
| `map_err` is the preferred typed wrap; chain break = un-preserved converter only | done |
| `Result<_, String>` antipattern (newtype error carrier, rolls into error-handling) | done |
| SNR: format! stringify, QuestionMark≠chain_break, type-graph advice, unique rollup | done |
| Dogfood: typed `CargoMetadata`/`json_parse`/`CliError`; `map_err(ctor)` preserve | done |
| SNR: test `.unwrap` / `.expect` are checklist (miette); `#[cfg(test)]` in `src/` is the test surface | done |
| Library Result-returning abort sites → `CordialError` (`From` wrap of associated error, including `fmt::Error` / `LexError`) | done |
| Binary: `cordial_cli` surfaces failures with miette (`src/boundary.rs`, linked from `main` only) | superseded — one crate, thin `main`, no `BinaryError` |
| Tests: harness abort sites → `miette::Result` + `into_diagnostic` / `ok_or_else` | done |
| `IrView::root`, `IrMut::insert_node`, `rebuild_path_index` return `CordialResult` | done |
| Dogfood panics: checklist **0**; tests **0** miette items; no inventory-only abort sites; abort-site action items **0** | done |
| Source-wrapper shape (`source` + file/line + `#[track_caller] fn new` that calls `Location::caller()`) | done |
| Error architecture suite (`Error` impls under `src/`; parent boxes Kind; native sources; recurse) | done |
| Dogfood: `CordialError` / `CliError` follow parent + boxed Kind + native sources; bin miette wrappers excluded | superseded — `CliError` folded into `CordialError` |
| One crate: CLI native sources on `CordialErrorKind`; `Cli::act`; miette only in `main` | done |
| Profile-specific `ErrorHandlingPolicy` layer gating in session | future |

---

## References

- [coverage-as-plugin.md](coverage-as-plugin.md)
- `src/plugin/error_handling.rs`, `src/plugins/error_handling.rs`
