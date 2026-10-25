# Per-file antipattern skip

Status: **Active**

## Problem

`amenable_kani::gallery` (in the amenable workspace) deliberately constructs
toy error shapes to find out what CBMC does with a given representation --
a bare `&'static str` struct field, an unused `_context`/`_buffer` parameter
that only exists to match a contract's harness signature. The "antipattern"
*is* the thing under investigation, not a defect: the file's own doc comment
says so, and the whole escalation ladder it belongs to exists to pin down
exactly which representations are cheap for Kani to model.

`ANTIPATTERN-STRUCT-STATIC-REF-001` and `ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001`
both fire correctly on these fixtures (real findings: `683eb4`, `3ad44e`,
`824998`, `0674d2`, `ad5426` on the static-ref rule; `49f998`, `291b88` on the
unused-arg rule, all under `crates/amenable_kani/src/gallery/`). The question
is how to record "this file's idiom is understood and intentional" without
either (a) `[antipatterns] enabled = false` for the whole crate, which would
also hide a real `Box<dyn Error>` or unnamed-contract-bound finding anywhere
else in `amenable_kani`, or (b) `cordial exceptions add` for each finding,
which works but grows the exceptions store for something architectural and
recurring-by-construction, not a one-off.

Checked `cordial explain antipatterns` and the real `cordial.toml` schema
empirically before filing this: the only opt-outs that exist today are
`[antipatterns] enabled = false` (crate-wide) and
`[antipatterns.static_refs] strategy` (workspace-wide remediation wording,
not scope). Neither is file-scoped. Tried `[antipatterns] skip_paths = [...]`
and `[rules."ANTIPATTERN-STRUCT-STATIC-REF-001"] skip_paths = [...]` against
amenable's real `cordial.toml` and reverted both -- no effect, confirming
neither key exists.

This should not need designing from scratch: the folder-or-file path-prefix
allowlist idiom already exists in three independent places --
`[modularity] generated_files`, `[visibility] mod_thin_skip`, and
`[tracing.stdio] skip_folders`. Antipatterns is the odd etiquette out.

## Design

Add `[antipatterns] skip_paths`, a map from rule id to a list of
crate-relative path prefixes (exact file or directory), reusing the matching
semantics `ModularityThresholds::is_generated_file` already implements
(`src/etiquettes/modularity/types.rs:93-101`: exact match or path-prefix
against the finding's crate-relative file) rather than the module-path
variant `mod_thin_skip_matches` uses (`src/etiquettes/visibility/enricher.rs:117`).

```toml
[antipatterns]
skip_paths = { "ANTIPATTERN-STRUCT-STATIC-REF-001" = ["src/gallery"], "ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001" = ["src/gallery"] }
```

Keyed by rule id, not one blanket path list, because the six antipattern
rules are independent signals: a gallery file that legitimately needs a bare
`&'static str` field for a Kani representation experiment is not thereby
exempt from `Box<dyn Error>` or unnamed-contract-bound checks too. This
mirrors `generated_files`'s own narrowness -- it exempts
`MODULARITY-FILE`/`MODULARITY-MODULE-SIZE` but leaves
`MODULARITY-TYPES-PER-FILE`/`MODULARITY-FUNCTION` live for the same files.

Matching: a finding's `file` (crate-relative, normalized the same way
`generated_files` and the exceptions store already normalize paths) matches
an entry under its rule id when it equals the entry exactly or has the entry
as a path-prefix component. Replacing `skip_paths` in `cordial.toml` replaces
the default (empty map); it does not union with it, matching
`generated_files`'s documented behavior.

Hook point: wherever `AntipatternsEtiquette`'s scan currently turns a
`preds.rs` match into a finding (`src/etiquettes/antipatterns/scan/preds.rs`,
`src/etiquettes/antipatterns/enricher.rs`), filter by `skip_paths` keyed on
that finding's own rule id before it's recorded -- the same seam
`is_generated_file` is consulted from inside the modularity enricher.

## Non-goals

- Not a replacement for `cordial exceptions`. A one-off, finding-specific
  exemption still belongs in the exceptions store. `skip_paths` is for an
  idiom that recurs by construction in a known location (a gallery/fixture
  directory, a generated-code tree) -- the same distinction `generated_files`
  already draws against the exceptions mechanism.
- Does not change `[antipatterns.static_refs] strategy`, which stays a
  workspace-wide remediation-wording setting, independent of this.
- Does not need default entries for `ANTIPATTERN-BOX-DYN-ERROR-001`,
  `ANTIPATTERN-STRING-ERROR-001`, or `ANTIPATTERN-VERSION-IN-MEMBER-001` --
  any rule id works through the same map, but there's no known motivating
  case for those three yet.

## Status

- [ ] Design
- [ ] `[antipatterns] skip_paths` config field + matcher
- [ ] Wire into the antipatterns scan/enricher seam
- [ ] `cordial explain antipatterns` opt-out text updated
- [ ] Tests
- [ ] Confirms the motivating case: amenable `amenable_kani::gallery`
      (`ANTIPATTERN-STRUCT-STATIC-REF-001` x5, `ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001`
      x2) scoped to `src/gallery` drops to zero findings with every other
      antipattern rule still live for that crate
