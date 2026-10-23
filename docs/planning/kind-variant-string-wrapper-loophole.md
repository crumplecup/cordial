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

## Required fix: follow error types through signatures

The direct conversion check is a useful early warning, but it does not close
the reported loophole. A helper can stringify a caught error and return a
message-only error. Follow the **types** across the called functions instead
of looking for particular expressions in their bodies. At each call, resolve
the callee signature, substitute generic parameters, and compare the input
error type with the error type it emits. If the emitted type has no reachable
source path to the input error type, that call broke the chain. This detects
the violation regardless of whether the helper uses `format!`, `to_string()`,
or another way to make text.

Reuse and extend the existing internal error type graph. Its `source_target`
edges are the starting point, but they need resolved type identities and a
verified `Error::source()` relationship before they can support this rule;
the current graph sometimes infers an edge from a field name. Build a
signature index from the shared source trees and resolve calls against it.
Ambiguous calls, trait-object erasure, or an unknown external source graph
must yield **unknown**, not an assumed pass. The rule stops at type
reachability: if the resolved output error type can carry the input error
through its source chain, it passes. If no such path exists, report a broken
chain. The rule does not require MIR, body-wide value-flow analysis, or proof
that a particular runtime value was retained.

For example, `fn wrap(err: IoError) -> AppError` can pass if `AppError` has a
source-carrying `Io(IoError)` variant, even if it also has a `Message(String)`
variant. A direct `err.to_string()` conversion can still be caught by the
existing local check, but this signature-level rule does not inspect which
variant the helper constructs. That is the deliberate limit of this lint.

### Acceptance cases

- A helper in another module returns `InvalidDescriptorSource::new(err.to_string())`;
  the caller's `map_err` is reported.
- Two or more helpers forward the error before one stringifies it; the caller
  is reported.
- A helper returns a type with a verified source path to the caught error
  type; the signature-level check passes.
- A helper accepts the error but drops it before returning a message-only
  error; the caller is reported.
- A broad return enum has both source-carrying and message-only variants;
  the signature-level check passes.
- An unresolved or opaque call produces an explicit unknown result, never a
  silent pass.

Use compiled fixtures for resolved-call cases. The existing source-snippet
fixtures do not resolve types or call targets and cannot validate this rule.
