# Proc-macro crates terminate the chain at `syn::Error`

Status: **Active**

## Problem

`internal_error_chain`'s compliance layer flags `ERROR-CHAIN-COMPLIANCE-STRINGIFY-001`
("converts an error to text without retaining its typed source") on two sites
in amenable's `amenable_derive` crate (a `proc-macro = true` crate):

- `crates/amenable_derive/src/verus_contract.rs:249` --
  `verus_predicate_body(&item_fn).map_err(|reason| syn::Error::new(predicate.span(), format!("predicate `{name}` {reason}, ...")))?`
- `crates/amenable_derive/src/witness/expand.rs:45` --
  `.build().map_err(|error| Error::new_spanned(input, error.to_string()))?` (`Error` is `syn::Error`, imported bare via `use syn::Error`)

Both are real instances of the pattern the rule is built to catch: a typed
foreign error (`PredicateBodyError`, `derive_builder`'s uninitialized-field
error) goes in, and only its `Display` text comes out, via `format!`/
`.to_string()`. In ordinary library code that's a real defect -- a caller
that wants to match on the cause, or walk `.source()`, can't.

Inside a `#[proc_macro_derive]`/`#[proc_macro_attribute]` expansion function,
though, `syn::Error` is not an intermediate carrier with a caller downstream
-- it is the terminus. The function returns `syn::Result<TokenStream>`; the
macro-dispatch shim converts an `Err` to `.to_compile_error()` and that's
what rustc shows the human who wrote the invalid derive input. There is no
later `.source()` walk to preserve a chain for, the same way a CLI's `main`
flattening its error to `Display` text for the terminal isn't a chain
violation either. `syn::Error` structurally cannot hold a secondary typed
source (it's a span + a message), so "wrap the foreign error inside the
`syn::Error` instead of stringifying it" is not an available fix at this
specific boundary the way it would be for, say, a crate's own `Kind` enum.

## Design

Add a narrow exemption to the compliance layer
(`src/etiquettes/error_ir/compliance_layer.rs`, the `on_map_err` /
`on_err_payload` methods that currently push
`InternalErrorComplianceId::StringifyForeign001` /
`DiscardTyped001`), gated on **both**:

1. **Crate kind.** `IrView::is_proc_macro(ctx.crate_name())` -- the fact
   already lives in the IR (`docs/planning/proc-macro-barrel.md`, Phase 1:
   `CrateKind::ProcMacro` from `cargo metadata`'s target kinds,
   `enricher/error_flow.rs` already reads it the same way instead of its
   old hand-rolled `[lib]` TOML scan). `SiteCtx` already carries
   `crate_name`; this is a lookup, not a new fact to plumb.
2. **The converter's result is a `syn::Error` construction.** Detected the
   same way `FOREIGN_TYPE_RULES` in `error_sites/foreign_infer.rs` already
   infers foreign types from snippet text, not type-checking: match a call
   path whose last two segments are `Error::new`, `Error::new_spanned`, or
   `Error::new2`. Both call shapes actually in use in amenable need to
   match -- `syn::Error::new(...)` (fully qualified, `verus_contract.rs`)
   and bare `Error::new_spanned(...)` (`witness/expand.rs`, where the file
   has `use syn::Error` or `use syn::{..., Error, ...}`) -- so the bare form
   needs the file's own use-imports consulted (already parsed per-file;
   `constructor_from_call` already extracts the call's path segments, this
   just adds "and did this file import `Error` from `syn`" alongside the
   fully-qualified check). A bare `Error::new` with no `syn` import in scope
   must not match -- that's what makes this a real signal and not just
   "any error type literally named Error".

When both hold, suppress `StringifyForeign001`/`DiscardTyped001` at that
site. Don't just vanish it -- record it as a contrast entry the way
`error_chain`'s own `ERROR-CHAIN-PRESERVED-QUESTION-MARK-001` /
`ERROR-CHAIN-PRESERVED-MAP-ERR-001` record "this site already does the
right thing" instead of silently passing over it. Add
`ERROR-CHAIN-COMPLIANCE-SYN-DIAGNOSTIC-001` to the reference/contrast pool:
a typed foreign error flattened into a `syn::Error` inside a proc-macro
crate is the chain's real, correct terminus.

## Why gate on both signals

`syn::Error::new`/`new_spanned` alone isn't enough: a non-proc-macro crate
can depend on `syn` for other reasons (a build script, a source-rewriting
tool) and construct a `syn::Error` without being at an actual
macro-expansion diagnostic boundary -- there, a caller further up might
still want the typed cause. The proc-macro crate-kind fact is what actually
establishes "there is no caller past this point for this specific error
type"; `syn::Error` can only ever become a `compile_error!` token stream
once it leaves a proc-macro crate's own public expansion functions, which
is exactly where rustc requires the `#[proc_macro*]` entry points to live
(crate root, per `docs/planning/proc-macro-barrel.md`).

## Non-goals

- Does not touch `ERROR-CHAIN-COMPLIANCE-ARCH-*` (parent/Kind/native-source
  architecture rules). Those are about a crate's *own* typed error graph; a
  proc-macro crate's internal helper functions that build and propagate
  their own typed errors (not the public `expand_*` → `syn::Error` boundary)
  still get the ordinary compliance checks.
- Does not exempt `Result<_, String>`
  (`ANTIPATTERN-STRING-ERROR-001`) or `Box<dyn Error>`
  (`ANTIPATTERN-BOX-DYN-ERROR-001`) carriers anywhere -- `syn::Error`
  specifically is the recognized terminal type here, not "any proc-macro
  crate gets to stringify freely".
- Does not add a config knob. Unlike the gallery path-skip request
  ([antipatterns-path-skip.md](antipatterns-path-skip.md)), this is a
  structural fact (crate kind + constructor identity), not a per-file
  idiom that needs an allowlist -- it should apply uniformly to every
  proc-macro crate in a workspace with no `cordial.toml` entry required.

## Status

- [ ] Design
- [ ] Detect `syn::Error::new`/`new_spanned`/`new2` construction (qualified
      and bare-imported) in the compliance layer's converter/result
      analysis
- [ ] Gate the exemption on `IrView::is_proc_macro`
- [ ] `ERROR-CHAIN-COMPLIANCE-SYN-DIAGNOSTIC-001` contrast id
- [ ] `cordial explain internal_error_chain` updated
- [ ] Tests
- [ ] Confirms the motivating case: amenable `amenable_derive` findings
      `13b1e4` (`verus_contract::expand_verus_predicate_witness`) and
      `d15e9a` (`witness::expand::expand_witness`) both drop to zero, with
      `internal_error_chain` still live for every other crate and for
      `amenable_derive`'s non-`syn::Error` sites
