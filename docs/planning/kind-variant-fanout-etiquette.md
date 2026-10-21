# Kind-variant fan-out: detecting kitchen-sink error variants

Design and implementation record for a compliance rule under the existing `ErrorHandling` plugin
family ([error-handling-as-plugin.md](error-handling-as-plugin.md)), in the
same "Error architecture" rule table as `ARCH-KIND-VARIANT-001` and
`SOURCE-SHAPE-001`.

---

## Problem

`amenable`'s `amenable_ext` crate has a `TemporalErrorKind::InvalidDescriptor`
variant wrapping a native source, `InvalidDescriptorSource { detail: String,
line: u32, file: String }`. Structurally, this is fully compliant with every
existing Error architecture rule: the variant is a 1-tuple of a native source
that implements `Error`, the source has `source`/owned `file`+`line`, the
constructor is `#[track_caller]`, and so on.

But in practice, this one variant is a disguised `Err(Error::Custom("..."))`
catch-all. In just two files
(`crates/amenable_ext/src/chrono/{civil,instant}.rs`) it's constructed from 17
distinct call sites, each with a completely unrelated free-text description:

```
"offset's hour component does not fit a u8"
"could not build a UTC offset descriptor"
"local date-time has no unique mapping under this offset"
"ISO weekday number is not in 1..=7"
"calendar date descriptor is not a real calendar date"
"month does not fit a u8"
"day does not fit a u8"
... (17 total, two files)
```

The same shape repeats via direct `InvalidDescriptorSource::new(...)` calls
across 9 more files in `amenable_ext`'s jiff backend. None of these 17+
failure modes are distinguishable by the type system — only by parsing
prose. A caller that wants to react differently to "hour overflowed" versus
"no unique local-time mapping" cannot; they're the same variant. That defeats
the entire point of a typed `Kind` enum, and it's invisible to every rule in
the current architecture table because none of them look at *fan-out* across
call sites — only at the shape of one construction.

(Real-world note: this exact pattern also turned out to interact badly with
Kani's DFCC contract-checking machinery — any heap allocation, including the
`String` inside a native source, reachable from a function called under
`#[kani::proof_for_contract]` analysis causes a CBMC out-of-memory failure
regardless of content or size, confirmed directly. That's a second, concrete
cost of the pattern, not the primary motivation — the primary motivation is
that it's a design antipattern independent of any verifier.)

---

## Why this is harder to detect than it sounds

A naive rule — "flag `KindVariant(NativeSource)` construction sites with
diverse literal arguments" — finds almost nothing here, because real call
sites don't construct the variant directly. Each file has one local helper:

```rust
fn invalid(detail: String) -> TemporalError {
    TemporalError::new(TemporalErrorKind::InvalidDescriptor(
        InvalidDescriptorSource::new(detail),
    ))
}
```

and every real call site is `invalid("some literal".to_owned())`. The actual
variant-construction expression appears exactly once per file, inside the
helper. Counting call sites of `TemporalErrorKind::InvalidDescriptor(` finds
one hit per file — the helper's own definition — and misses the fan-out
entirely.

---

## Proposed detection strategy

Don't try to see through the wrapper semantically. Exploit its shape instead.

**Step 1 — identify "string-in, fixed-variant-out" wrapper functions.**
A function (any visibility) whose signature takes exactly one meaningful
`String`/`impl Into<String>`/`&str` parameter, and whose body is a single,
unconditional expression constructing exactly one fixed `Kind` variant (or
calling exactly one native-source constructor) using that parameter — no
branching on the string's content, no other variant reachable from this
function. This is a purely syntactic shape check against the AST the
`error_ir` visitor already walks; it doesn't need to know `InvalidDescriptor`
or `InvalidDescriptorSource` exist by name.

**Step 2 — treat the wrapper (if found) as the aggregation anchor; otherwise
use the native-source constructor directly.** If a call site invokes a
Step-1 wrapper, trace exactly one hop to find what it forwards to (already
established by Step 1's shape check). If there's no wrapper and callers hit
`NativeSource::new(literal)` directly, that constructor is itself the anchor
— no extra tracing needed. This bounds the analysis to one hop of
indirection, not general interprocedural dataflow.

**Step 3 — collect, per anchor, every call site's literal string argument**
(a string literal, or `.to_owned()`/`.into()`/`.to_string()` applied directly
to one) across the crate's reachable library source files. The shared parser
cache still serves the same syntax trees to other scanners in the run.

**Step 4 — flag when call-site count and literal diversity cross a
threshold.** The default threshold is more than three distinct call sites
with more than three distinct literal texts. Set
`[internal_error_chain] kind_fanout_max_call_sites = 0` to flag the first
matching literal call site; this project uses zero. The finding points at
the `Kind` variant **definition**, when it is in the scanned crate, as the fix site (split
into more variants, most of which won't need a `String` field at all — many
of the 17 examples above are plain preconditions with no dynamic content,
e.g. `MonthOutOfRange`, `HourComponentOverflow` as unit variants). When the
variant is defined in another crate, the finding points at the local wrapper
or first call site. Direct source calls without a visible Kind are grouped by
source constructor and reported for review, without asserting that they all
feed one variant. The call sites are listed as evidence.

Proposed rule ID, fitting the existing table's namespace:

| Rule | When |
| --- | --- |
| `ERROR-CHAIN-COMPLIANCE-ARCH-KIND-FANOUT-001` | A `Kind` variant's native-source constructor (direct, or via a one-hop string-in/fixed-variant-out wrapper) is called from more than N distinct sites with syntactically diverse literal-string arguments |

The architecture catalog has its own AST walk, separate from the `error_ir`
visitor, and both use the same cached parsed syntax tree. The fan-out collector
runs during that existing architecture walk; it adds no parse or traversal.

---

## Known limitations

- **Heuristic, not a semantic judgment.** The rule cannot distinguish "these
  17 things are secretly one error" (true positive) from "these N things are
  legitimately one error family that doesn't need finer variants" (needs a
  human, and cordial's exceptions mechanism, to adjudicate per case).
- **One hop of indirection only**, deliberately. Chasing wrapper-of-wrapper
  chains generally reintroduces the same interprocedural-dataflow complexity
  this design avoids. If a real case needs more hops, that's a signal the
  wrapper itself is doing too much, which is worth flagging on its own
  terms rather than chasing through it.
- **Threshold tuning.** Three is the default, not derived from data. Projects
  can set `kind_fanout_max_call_sites` under `[internal_error_chain]`; zero
  outlaws the pattern entirely.
- **Per-crate aggregation.** The architecture catalog is built per crate, so
  call-site totals do not yet combine separate workspace members. Imported
  Kind variants can still be detected from call expressions within one crate.
- Taken pedantically, fixing a flagged case costs real boilerplate (more
  variant names, more match arms) in exchange for real distinguishability.
  That tradeoff is the point. Cordial currently has no separate review
  disposition: enabled findings are open, as with `feature_warnings`. The rule
  asks for review and can use the normal documented exception mechanism.

---

## References

- [error-handling-as-plugin.md](error-handling-as-plugin.md) — the `Kind`
  architecture rule table this extends, and the `error_ir` visitor this reuses.
- Motivating case: `amenable_ext::chrono::{civil,instant}`'s `invalid()`
  helper and `TemporalErrorKind::InvalidDescriptor`; the same shape recurs via
  direct `InvalidDescriptorSource::new` calls across `amenable_ext::jiff::backend::{parser_helpers,parser_exchanges,interval,zone_conversions,instant,time_interval,conversion,zone_factory,duration}`.
