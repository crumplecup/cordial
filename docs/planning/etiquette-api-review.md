# Etiquette API and pipeline review

**Status:** Active. Work through the checklist in order, updating this page as
each slice lands. The order reflects the 2026-10-20 review of the writing
etiquettes series and the pipeline behind it.

## Priority checklist

1. [x] **Preserve caller locations in error conversions.** Correct guidance
   recommending `map_err(Type::from)` when the conversion records
   `Location::caller()`. Add a targeted compliance finding, regression tests
   for `?`, a closure, and a function item, and repair Cordial's own affected
   calls. The detailed checklist is in
   [error handling as plugin](error-handling-as-plugin.md#priority-correction-caller-locations-through-map_err).
2. [x] **Measure and remove repeated source work.** Record parse counts and
   durations by file and hook during a combined run, including repeated reads
   and walks of the same file. Use hotpath for the expensive call paths and
   explicit counters for repeated `(file, hook)` operations. Then share source
   or parsed facts where measurements show useful overlap. Preserve each
   scanner's source-tree and fixture rules, and verify finding parity. Do not
   assume that a shared graph already means one `syn` parse per file.
3. [x] **Resolve the `Probe::interests()` contract.** Remove the unused
   declaration. Probes select nodes with `IrView::nodes_matching()` when they
   need a query; root- and inventory-driven probes do not declare a dummy
   `ALL_NODES` query. The custom plugin example exercises the simpler contract.
   Measure query count and time before considering a central candidate API.
4. [ ] **Give plugins a typed path through their own facts.** Keep the graph's
   cross-plugin boundary dynamic, but provide fallible typed helpers for a
   plugin's serialized attributes and required finding fields. Prefer the
   node's span to duplicate file and line attributes where possible. Review
   whether reporters can receive findings scoped to their etiquette. Missing
   required data should fail visibly instead of becoming line `0`, an empty
   field, or a fabricated context.
5. [ ] **Correct the writing etiquettes series and example.** State that
   exceptions are applied centrally and shared hooks are deduplicated by id;
   explain hook order and any stock enrichers the example actually needs.
   Narrow the "parsed once" claim to the behavior implemented after item 2.
   Clarify probe id versus marker label, use checked facts in the example,
   escape CSV, and describe custom-plugin opt-out accurately. Keep the
   compiled example and tutorial in sync.
6. [ ] **Polish registration ergonomics.** Replace ambiguous positional
   arguments and the quality boolean with named or typed choices. Explore
   deriving explain-page rules from registered rules and reducing repeated
   rule ids. Keep this last unless an earlier change needs it.

## Evidence and decision boundaries

- The latest completed combined run recorded in the default store's
  `ledger.json` took **16.79 s**: enrichers accounted for **14.84 s** and
  probes for **0.036 s**. These are hook totals from that run, not a general
  benchmark. The one-etiquette-at-a-time `timings.json` report must not be
  summed to estimate a combined run. See [pipeline timings](../PIPELINE_TIMINGS.md)
  for the ledger and hotpath formats.
- Hooks shared by etiquettes are already deduplicated by id in
  [`session/resolve.rs`](../../src/session/resolve.rs). This prevents the same
  registered enricher from running once per etiquette. It does not prevent
  distinct enrichers from parsing overlapping source files.
- [`SourceLoadView::populate_ir`](../../src/loader/populate.rs) and
  [`AttributeEnricher`](../../src/enricher/attribute.rs) previously parsed
  source independently. Scanners such as
  [`allows`](../../src/etiquettes/allows/scan.rs) and
  [`panics`](../../src/etiquettes/panics/scan/source.rs) also read and parsed
  overlapping files. The unified error IR scan already shared a parse across
  some error-handling layers; that gain is preserved.
- In a combined quick-tier run on 2026-10-20, per-file counters found 691
  distinct Rust files, 2,670 reads, and 3,267 `syn` parses across these five
  hooks. Parse time summed to 2.422 seconds. With the shared parsed-tree cache,
  the same run shape read and parsed each of those 691 files once (0.586
  seconds of parse time); later hooks reused the tree. The source loader still
  owns the `src/` snapshot, while each scanner keeps its existing tree and
  fixture selection and its own AST walk. The optimized run's full wall time
  was 14.26 seconds, with 13.23 seconds in load and probe; these wall times
  reflect different runs and are not a controlled speedup estimate. Hotpath
  attributed 0.589 seconds to `source_work::parse` in the optimized run.
- `Probe::interests` was required by the trait but never read by
  [`load_and_probe`](../../src/session/load.rs). Of 36 built-in implementations,
  32 queried nodes inside `probe`; the other four worked from the crate root
  or an external inventory. The latest build-tier run spent 36 ms across 26
  active probes, out of 30.8 seconds total. This includes all probe work, so
  a central query engine offered little demonstrated value. The declaration
  was removed; [`nodes_matching`](../../src/ir/view.rs) remains available to
  probes that need it.
- Exceptions are already applied after assessment in
  [`session/assess.rs`](../../src/session/assess.rs). The series should teach
  this behavior rather than asking plugin assessors to implement it.
