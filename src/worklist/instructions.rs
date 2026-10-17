//! Compiled guidance for the agent worklist commands.

/// The release 1 worklist loop, suitable for an agent instruction file.
pub const WORKLIST_INSTRUCTIONS: &str = r#"How to work through cordial's open items

1. Run `cordial open`. It shows the top item, and builds the list if there isn't
   one yet. Building can take a few minutes on a large project, so run it with a
   long timeout.
2. Fix the item, run the project's own focused checks, then run
   `cordial done <id>` using the id on the item's header line. This checks it
   off and shows the next item.
3. If you cannot act on an item now, run `cordial skip <id>`. It stays
   unchecked and the cursor moves on.
4. When cordial says the list is complete, run `cordial submit`. It reruns the
   gate, tells you which fixes held, and builds a fresh list.

The work is finished when `cordial submit` reports no open items. Do not report
it complete before then. `cordial submit` checks claims; `cordial done` only
records them.

You can work out of order. The id names the item, so fix several items together
and run `cordial done <id> <id> <id>`. Running done twice on the same id is
harmless. Without an id, done and skip act on the current item. When sending
several commands at once, always name the ids.

Run `cordial submit` on its own and wait for it. Another cordial command will
wait for submit to finish and then see the new list.

`cordial peek` shows the current item again. `cordial stats` shows counts by
area; add `--verbose` to see items, and `--area <title>` to narrow the view.

Some findings are intended. Run the `cordial explain` command named by the item
to learn how to document an exception. Once documented, run done and let submit
confirm that the finding disappeared.

Use done and skip as you go. They are cheap; submit is a full gate, so run it
once per batch and at the end. Skipped items return on the next list.

Item locations are hints. Line numbers shift as you edit, so search for the
symbol named in an item. If your change removed the code, run done and let
submit confirm. If the item no longer makes sense, skip it for now.

A line starting with NEXT: names the next step.
"#;

pub(crate) const OPEN_HELP: &str =
    "Show the current item; build a list if needed. See `cordial instructions` for the full loop.";
pub(crate) const PEEK_HELP: &str = "Show the current item again without running the gate.";
pub(crate) const DONE_HELP: &str =
    "Claim that named items, or the current item, are fixed. Submit later to verify.";
pub(crate) const SKIP_HELP: &str =
    "Set named items, or the current item, aside until the next list.";
pub(crate) const SUBMIT_HELP: &str = "Rerun the gate, score claims, and build a fresh list.";
pub(crate) const STATS_HELP: &str =
    "Show list counts by area; use --verbose to see individual items.";
pub(crate) const INSTRUCTIONS_HELP: &str = "Print the agent worklist loop and verification rules.";
