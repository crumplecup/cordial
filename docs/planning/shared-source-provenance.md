# Shared source provenance

**Status:** Active design. Implement in small slices; run focused checks and a
zero-open Cordial gate before each phase commit.

## Invariant

Every outer Rust file analyzed by a production source hook must use the parsed
tree issued by that session's shared source view. A scanner must not be able to
submit an independently parsed `syn::File` as if it came from that view. The
source text, path, syntax tree, and provenance travel together in one type with
private fields and no public constructor. A separate token passed beside raw
data would permit the data and token to be mismatched.

The current cache is useful but does not enforce this invariant. `SourceLoadView`
returns a cached `Rc<syn::File>`, while many production scanners still accept
`&str` and `&Path` and call `syn::parse_file` themselves. Source hooks receive
only `&dyn LoadView`, so they can bypass the cache without changing a type
signature. See [`SourceLoadView`](../../src/loader/source.rs),
[`IrEnricher`](../../src/hooks/enricher.rs), and the
[`modularity` scanner](../../src/etiquettes/modularity/scan.rs) for one such
route.

## API shape

- The source view issues an opaque `SharedParsedFile` (name provisional) for a
  requested path. It owns or retains the exact source text and `syn::File` from
  one cache entry, plus the path and a session provenance brand. Its fields and
  constructors stay private to the loader. The view alone creates it.
- Production source scanner entry points take `&SharedParsedFile`, or a narrower
  view derived from it, rather than separate source, path, and syntax arguments.
  They obtain the source, path, and syntax through accessors on that same value.
  The source hook interface should make this value the normal input so a plugin
  author does not have to downcast `LoadView` and choose a parser.
- Facts that claim a source location should be derived from this file view.
  If facts must cross into the dynamic graph, provide a checked conversion at
  that boundary; avoid accepting an unrelated path or span alongside the file
  brand. A per-file brand or a scoped callback may be needed to prevent a node
  from one file being paired with another file's path. Choose this after
  prototyping the smallest useful scanner interface.
- Keep the shared parse lazy. The source loader already snapshots `src/`; scans
  may also visit `tests/` and other roots. A first request reads and parses a
  path, and later requests in that session reuse the same entry. Keep the
  current thread-local ownership constraint of `syn::File` in view when shaping
  the hook API.
- Parsing embedded Rust strings or macro-specific token grammars is a separate
  operation. It must have an explicit source category rather than masquerade
  as an outer file obtained from the shared view.

## Migration

1. Introduce the sealed parsed-file carrier and change the shared cache to
   return it. Prove that callers cannot construct one from a raw `syn::File` or
   pair it with an unrelated source/path. Keep the existing cached call sites
   working through its accessors while defining the source-hook entry point.
2. Migrate production source scanners by family. At each entry point, replace
   raw source/path/syntax parameters with the carrier, then remove independent
   outer-file reads and parses from that path. Retain dedicated standalone
   helpers for tests or external use only if they cannot feed the production
   scanner with unbranded syntax. Preserve each scanner's file selection,
   fixture exclusions, and finding semantics.
3. Move source-derived location construction behind the carrier where needed.
   Audit graph attributes and finding conversion paths for raw paths or spans
   that can be paired with another file's syntax. Use typed, fallible
   conversions for required facts instead of defaults that hide a broken
   provenance link. This overlaps the typed-facts work in the
   [etiquette API review](etiquette-api-review.md).
4. Remove production APIs that accept plain `syn::File` or source text for an
   outer file. Update the custom-plugin example and writing-etiquettes guidance
   to teach the enforced source-hook contract, then narrow or restore their
   "parsed once" claim to the measured behavior.

## Acceptance checks

- Compile-fail coverage shows that a plugin cannot construct the carrier, use
  an independently parsed tree at a production scanner boundary, or swap a
  file's provenance with another file's source data.
- Instrumented combined runs report at most one shared outer-file parse per
  `(session, path)`, including paths reached by multiple etiquettes. Compare
  per-file parse counts and elapsed time with the existing source-work and
  hotpath output; do not infer this from a cache hit count alone.
- Focused scanner tests and a before/after finding comparison show that file
  selection and finding payloads have not drifted. Complete the relevant
  Cordial tier with zero open items before each phase commit.
- A source audit finds no direct outer-file `syn::parse_file` use in production
  scanners. Any remaining direct parse has an explicit embedded-source or
  standalone-test purpose.

## Open design decision for the first slice

Prototype the carrier against one representative scanner, such as modularity,
before choosing whether `IrEnricher` needs a source-specific companion trait or
whether `EnrichView` should expose a typed source cursor. The chosen interface
must make provenance-bearing source access the default for plugin authors; a
mere helper method on the current untyped hook is insufficient.
