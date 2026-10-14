# Your First Etiquette, Part 7: Make It Yours

*Part 7 of 7. Previous: [When order matters](writing-etiquettes-6-order.md).*

Over the last six posts we built the `todo!()` lint from [Part 1](writing-etiquettes.md)
from a rule name to a CSV, ran it, and gave it a place in the queue. This last
post is about taking it further. The lint has a handful of parts that change,
and the rest is scaffolding, so most of what follows is a short list of ways to
bend it.

*The numbered comments in the code match the notes after each block, as
described in [Part 1](writing-etiquettes.md#how-the-series-works).*

## Variations

- **Flag something else.** Change the macro name in the visitor, the snippet
  text, the id, the label, and the strings. That is a different lint with the
  same hooks. `unimplemented!()` and `dbg!()` are both small changes of this
  kind.
- **Add a rule.** One id per kind of problem. A second rule for `unimplemented!()`
  gets its own `Rule`, its own line on the explain page, and its own handle for
  exceptions.
- **Pair it.** A lint that only pushes one way tends to be satisfied by a
  neighboring shortcut. The [error-handling deep dive](error-handling-deep-dive.md)
  is about arranging lints so that each shortcut meets another lint pulling the
  other way.
- **Order it.** If fixing your rule changes what another rule sees, add an
  `After` row, as in [Part 6](writing-etiquettes-6-order.md).
- **Make it count in the report.** This one needs more than a sentence, so it
  gets its own section below.

## Making it count in the report

`cordial quality` writes `quality-report.md`, a checklist of the open action
items across a project, and `--deny-open` fails the run while any remain. Until
now our lint's findings went only to its own CSV. Wrapping the etiquette in
`StaticQualityEtiquette` with a `QualityAreaSpec` adds a row for it to that
report, so its findings count toward the gate. The built-in
`glob_imports` in `src/etiquettes/glob_imports.rs` is a compact example: a
visitor, a probe, an assessor, and three reporters that write a CSV, a
checklist, and a summary.

To wrap the etiquette, move the body of the earlier `LazyLock` closure from
[Part 4](writing-etiquettes-4-writing-and-bundling.md) into a function,
`todo_etiquette()`, and build the quality etiquette around it:

```rust
pub static TODO_QUALITY_ETIQUETTE: LazyLock<StaticQualityEtiquette> = LazyLock::new(|| {
    StaticQualityEtiquette::new(
        // 1. The etiquette from Part 4, unchanged.
        todo_etiquette(),
        // 2. `Some` adds a row to `quality-report.md`.
        Some(QualityAreaSpec::new(
            // The row's heading.
            "Leftover todos",
            // The artifacts the row links to.
            "acme-todo.checklist.md",
            "acme-todo-summary.md",
            // 3. How to count the row.
            quality_area_compute,
        )),
    )
});

// 3. Count the open findings in our category and describe the count
//    in one line.
fn quality_area_compute(findings: &[&dyn Finding]) -> (usize, String) {
    let open = count_open_category(findings, "acme_style");
    (open, format!("leftover `todo!()` sites **{open}**"))
}
```

1. The wrapped etiquette is the one you already built.
2. `Some(QualityAreaSpec)` is what puts the lint in the report. The checklist and
   summary names point at artifacts that a checklist reporter and a summary
   reporter write, in the same way the CSV reporter writes its file. Use `None`
   only for a reference inventory, or when the findings are intentionally rolled
   into another area.
3. The compute function turns findings into a count and a summary line. The
   category string is the one `TodoRule::category` returns.

Register `TODO_QUALITY_ETIQUETTE` in the plugin's etiquette slice in place of
`TODO_ETIQUETTE`.

## Wrapping up

That is the whole path. A rule gives findings a name. An enricher puts facts in
the graph, a probe notices them, an assessor judges them, and a reporter writes
them down. An etiquette and a plugin bundle those hooks so cordial can run them,
and an ordering row says where the lint belongs among the others.

Where to go from here:

- The [reference page](writing-etiquettes-reference.md) covers the plugin shapes
  this series didn't need and the rules for stable identifiers.
- The [error-handling deep dive](error-handling-deep-dive.md) shows what a set of
  lints looks like when they are designed to work against each other.
- [Built-in etiquettes](built-in-etiquettes.md) is a tour of what ships with
  cordial, and `cordial explain` prints the compiled-in explanation for each.

---

**The series: Your First Etiquette.**
[1. The plan and the rule](writing-etiquettes.md) ·
[2. Finding the sites](writing-etiquettes-2-finding-sites.md) ·
[3. Noticing and judging](writing-etiquettes-3-noticing-and-judging.md) ·
[4. Writing it down and bundling it up](writing-etiquettes-4-writing-and-bundling.md) ·
[5. Taking it for a run](writing-etiquettes-5-running.md) ·
[6. When order matters](writing-etiquettes-6-order.md) ·
**7. Make it yours** ·
[Reference](writing-etiquettes-reference.md)
