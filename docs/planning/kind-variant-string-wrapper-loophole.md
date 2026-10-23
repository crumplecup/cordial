# Stringified error source chain gap

Status: **Partial — direct conversions detected; call-chain analysis pending**

## Reported symptom

`amenable_time::error::InvalidDescriptorSource` and
`ParseRejectedSource` can receive `format!("{err}")` or `err.to_string()`
from a caught error. The new string becomes the native source's message,
and the original error is no longer available through `Error::source`.

For example, `builder.build().map_err(|err| InvalidDescriptorSource::new(
format!("could not build: {err}")))` was not flagged. Cordial inferred
foreign error types from a small set of known receiver expressions before
running `StringifyForeign001`; it cannot infer the type of `builder.build()`
from syntax alone. That early return was the immediate escape hatch.

## Separate fanout policy

The maintainer requested `kind_fanout_max_call_sites` to default to zero.
This is a valid frequency policy in its own right. It flags matching literal
call sites regardless of how the error payload is represented, so it cannot
enforce preservation of a typed source chain. The default is now zero.

## Current coverage

Inspect the error binding at conversion sites before the receiver-type
inference guard. A `map_err` closure, `if let Err(...)`, or `match` error arm
that converts its bound error with `format!` or `.to_string()` and does not
pass that binding into the resulting error gets `StringifyForeign001`.
Known typed-error constructors retain the existing `DiscardTyped001` finding.
The rule id stays stable. Regression fixtures cover an uninferable receiver,
the three conversion forms, and a wrapper that retains the typed source.

The original proposal was to reject every native-source struct whose
content fields are all `String`. That shape alone does not establish that
an earlier error was stringified: a fresh invariant error may legitimately
carry a message. The conversion site supplies the evidence for this rule.

## Required fix: follow the value through calls

The direct conversion check is a useful early warning, but it does not close
the reported loophole. A helper can stringify a caught error and return a
message-only error. Conversely, passing the caught error as an argument does
not prove that the returned error exposes it through `Error::source()`. Neither
case can be decided from the spelling of a call or the name of a field.

Build this rule on compiler-resolved types and control flow. Begin at each
caught `Err` value, follow assignments, branches, closures, and calls, and
summarize local functions in terms of which input error values can reach the
returned error's `source()` chain. Compute summaries to a fixed point for
recursive calls. Check every return path; a message derived from the caught
error is useful diagnostic evidence, but loss of the typed source is the
violation even when no explicit `format!` or `to_string()` appears at the
boundary. Follow the actual `Error::source()` implementation for local error
types rather than treating a `source` field or constructor argument as proof.

Record three outcomes for each conversion: **preserved**, **lost**, and
**unknown**. An unresolved call, dynamic dispatch target, external body, or
unprovable source implementation must remain unknown; it must not silently
count as preservation. Decide how unknowns surface in the worklist before
enabling the semantic rule by default. Keep the existing direct check only as
an explicitly partial signal while this backend is built.

The current shared `syn` tree can locate candidate boundaries without another
parse, but it does not supply resolved calls or MIR. Prototype a compiler
analysis adapter using a pinned toolchain and a separate process, then measure
its cost before assigning a tier. Rustc's
[`after_analysis` callback](https://rustc-dev-guide.rust-lang.org/rustc-driver/interacting-with-the-ast.html)
exposes type information, and
[MIR](https://rustc-dev-guide.rust-lang.org/mir/index.html) provides the
control-flow representation needed for the summaries. Its
[internal API](https://rustc-dev-guide.rust-lang.org/rustc-driver/external-rustc-drivers.html)
requires matching compiler components and is unstable, so the prototype must
establish a supported toolchain and failure policy before it enters the normal
pipeline.

### Acceptance cases

- A helper in another module returns `InvalidDescriptorSource::new(err.to_string())`;
  the caller's `map_err` is reported.
- Two or more helpers forward the error before one stringifies it; the caller
  is reported.
- A helper formats a message and returns an error whose `source()` exposes
  the caught error; the caller passes.
- A helper accepts the error but drops it before returning a message-only
  error; the caller is reported.
- A branch preserves the source on one path and loses it on another; the
  losing path is reported.
- An opaque external or dynamic call produces an explicit unknown result,
  never a silent pass.

Use compiled fixtures for these cases. The existing source-snippet fixtures
do not resolve types or call targets and cannot validate this rule.
