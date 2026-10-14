# Your First Etiquette, Part 6: When Order Matters

*Part 6 of 7. Previous: [Taking it for a run](writing-etiquettes-5-running.md).
Next: [Make it yours](writing-etiquettes-7-make-it-yours.md).*

The `todo!()` lint from [Part 1](writing-etiquettes.md) works as of
[Part 5](writing-etiquettes-5-running.md), and it works without any opinion
about when its findings should be dealt with. Many lints need none. This post is
for the ones that do, and it is optional: if your lint's findings can be fixed
in any order, skip ahead to [Part 7](writing-etiquettes-7-make-it-yours.md).

*The numbered comments in the code match the notes after each block, as
described in [Part 1](writing-etiquettes.md#how-the-series-works).*

## Why order matters

Some lints change what other lints see. Fixing one can make another's findings
disappear, or appear. Cordial lets a lint say which lints come first.

It matters because a report is a work queue. Someone working down it, whether a
person or an agent, should meet the prerequisite fixes first, and should not do
work that a later fix undoes.

## A row for our lint

The ordering is a table of rows over rule ids. Each row says "this rule comes
after these rules, and here is why". Our lint has a natural place in it. Cordial
ships a family of derive rules, which flag hand-written constructors and
accessors that a derive macro could generate. When one of those is replaced, a
`todo!()` sitting inside it goes away with it, so leftover todos should be
judged after the derives. The row looks like this:

```rust
use cordial::{DERIVE_RULE_IDS, LintConstraint, OrderExplain};

pub const ROWS: &[LintConstraint] = &[LintConstraint::new(
    // 1. The rule being placed.
    "ACME-TODO-001",
    // 2. The rules it must follow.
    DERIVE_RULE_IDS,
    // 3. The reason.
    TODO_AFTER_DERIVES,
)];
```

The reason is an `OrderExplain`, a title and a body:

```rust
const TODO_AFTER_DERIVES: OrderExplain = OrderExplain::new(
    // 4. A title for the edge.
    "Leftover todos after derive patterns",
    // 5. Why this rule has to wait.
    "Derive replaces hand-rolled constructors and accessors, and a todo!() \
     inside one goes away with it. Judge the todos that are left once the \
     derives are done.",
);
```

1. A row places one rule id. Give each rule its own row, in keeping with one id
   per kind of problem.
2. The predecessors are a list of ids. `DERIVE_RULE_IDS` is cordial's list of
   every derive rule, so one row covers all of them. Your own rows can name
   built-in ids and other plugins' ids alike.
3. The reason is a constant, so many rows can share one.
4. The title names the edge.
5. The body says why the order is right, and `cordial explain` shows it to the
   reader.

## After and Before

Behind `LintConstraint` is a small trait. You write the `After` side, and
`LintConstraint` already implements it, though any type of your own can too:

```rust
pub trait After {
    // The rule ids that must be resolved first.
    fn after(&self) -> &[&str];
    // Why this rule follows them.
    fn explain(&self) -> OrderExplain;
}
```

The other direction, `Before` ("this rule runs before these rules"), comes for
free. It is derived from the `After` rows, so nothing has to be written twice.
Implementing `After` gets you both.

## How rows get merged

Cordial keeps its own built-in rows in a compiled table. Your plugin's rows are
loaded when the session starts, and cordial merges them with the built-in ones
into a single order. The merged table is checked as a whole: a row that names an
unknown id is an error, and so is a cycle, because "A after B after A" has no
first step.

The merged order shows up in two places. `cordial explain acme-todo` gains a
"Resolution order" section with a "Runs after" line for the derive rules, and the
derive etiquettes' pages gain a matching "Runs before" line pointing at
`ACME-TODO-001`. And once the lint has a row in the quality report, as in
[Part 7](writing-etiquettes-7-make-it-yours.md), its area is placed after the
derive areas, so a reader working down the report reaches the derives first.

## Cordial's own chain

Cordial's built-in rows form a chain, and every link has a reason. They are a
good model for writing yours:

| Rule family | Follows | Why |
| --- | --- | --- |
| tracing | derives | the derives delete what tracing would instrument |
| modularity | inline tests | moving tests changes line counts, which can change which modularity lints fire |
| cfg hygiene | cfg scatter | lock down cfg hygiene after the flags have stopped moving |
| doc warnings | feature warnings, dependency freshness | gating items by feature changes what rustdoc sees |

The last row shows that the table is a graph and not a single line. A rule can
wait on more than one predecessor.

## What we have so far

The lint now says where it belongs in the queue, with a reason that `cordial
explain` can show. If your next lint needs the same treatment, the shape is the
same: one row, one reason, and the `After` side is the only one you write.

One post left. It collects the variations, and finishes by giving the lint a row
of its own in the quality report.

**Next: [Part 7, Make it yours](writing-etiquettes-7-make-it-yours.md).**

---

**The series: Your First Etiquette.**
[1. The plan and the rule](writing-etiquettes.md) ·
[2. Finding the sites](writing-etiquettes-2-finding-sites.md) ·
[3. Noticing and judging](writing-etiquettes-3-noticing-and-judging.md) ·
[4. Writing it down and bundling it up](writing-etiquettes-4-writing-and-bundling.md) ·
[5. Taking it for a run](writing-etiquettes-5-running.md) ·
**6. When order matters** ·
[7. Make it yours](writing-etiquettes-7-make-it-yours.md) ·
[Reference](writing-etiquettes-reference.md)
