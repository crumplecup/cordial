# Marginal cost of Cordial checks

The goal is to estimate how much each etiquette and each rule adds to a real
gate, by pipeline stage. Standalone cost and leave-one-out cost answer different
questions. Keep both; use pair experiments to investigate interactions between
checks, never as a replacement for leave-one-out.

## Measurements

- Standalone: run one etiquette with an initially empty Cargo target, then run
  it again with that target warm.
- Marginal: run the full selected set and the same set without one etiquette.
  Give each side its own initially empty Cargo target, then repeat both warm.
  Report signed full-minus-without wall and stage deltas.
- Overlap: standalone minus marginal. This is shared infrastructure plus
  interactions and measurement noise. It is not an additive allocation of
  shared cost; marginal estimates need not sum to total run time.
- Use one binary and fixed project/configuration for runtime comparisons.
  Feature-isolated binaries measure build cost separately.

The `phase` rows are inclusive of `pipeline` and hook rows. Compare the same
stage across passes; do not sum rows. Cargo subprocess time appears at its
parent hook. Cold means an empty Cargo target, not a cold filesystem cache or
an empty Cordial project store.

## Worklist

- [x] Keep standalone cold/warm measurements.
- [x] Add opt-in etiquette leave-one-out cold/warm measurements and stage deltas.
- [x] Record rule IDs with their owning etiquette without mislabeling bundle
  timings as individual rule timings.
- [ ] Record IR node counts and serialized bytes at stage cut points, alongside
  parse/cache counters, to explain why an omission changes downstream work.
- [ ] Make rule execution selectable at the rule boundary. A multi-rule
  assessor must skip the excluded rule's actual work, while retaining shared
  loading and enrichment needed by other rules. Then apply the same paired
  leave-one-out experiment to each rule ID.
- [ ] Add repeated, interleaved warm comparisons and uncertainty summaries for
  noisy gates, especially those dominated by Cargo subprocesses.
- [ ] Use pair experiments on high-overlap or surprising results to identify
  which hooks and IR facts are shared.
