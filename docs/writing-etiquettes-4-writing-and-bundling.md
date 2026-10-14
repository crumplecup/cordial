# Your First Etiquette, Part 4: Writing It Down and Bundling It Up

*Part 4 of 7. Previous: [Noticing and judging](writing-etiquettes-3-noticing-and-judging.md).
Next: [Taking it for a run](writing-etiquettes-5-running.md).*

By the end of [Part 3](writing-etiquettes-3-noticing-and-judging.md) the
`todo!()` lint from [Part 1](writing-etiquettes.md) could turn every `todo!()`
into an open finding, but a finding that nobody can read is not much use, and
cordial still doesn't know the lint exists. This post fixes both. We write the
last hook, the reporter, and then bundle all the hooks into an *etiquette* (a
named bundle of checks) and a *plugin* (the product you register with cordial).

*The numbered comments in the code match the notes after each block, as
described in [Part 1](writing-etiquettes.md#how-the-series-works).*

## Writing it down

The reporter sees only judged findings, and its output is an artifact written to
the project store.

```rust
impl Reporter for TodoCsvReporter {
    fn id(&self) -> &str {
        "acme-todo-csv"
    }

    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn cordial::Artifact>>> {
        // 1. Every finding in the run arrives here.
        let findings = view.findings();

        let mut body = String::from("crate,context,file,line,snippet\n");
        for finding in findings {
            // 2. Keep only our rule.
            if finding.rule().id() != "ACME-TODO-001" {
                continue;
            }

            // 3. Read the finding's fields back by name.
            let mut sink = cordial::MapFindingSink::default();
            finding.emit(&mut sink);
            let field = |name: &str| {
                sink.fields()
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default()
            };

            // 4. Write one CSV row.
            body.push_str(&format!(
                "{},{},{},{},{}\n",
                field("crate"),
                field("context"),
                field("file"),
                field("line"),
                field("snippet"),
            ));
        }

        // 5. Wrap the text as an artifact.
        Ok(vec![Box::new(TextArtifact::new(
            "acme-todo.csv".to_string(),
            "text/csv".to_string(),
            body,
        ))])
    }
}
```

1. A reporter gets every finding in the run, whichever rule produced it.
2. The rule id picks out the findings this reporter cares about.
3. `emit` is the contract between findings and reporters, so the reporter never
   needs the concrete finding type.
4. The body is plain CSV, one line per finding.
5. An artifact is a file name, a media type, and contents. Cordial writes it to
   the project store, which by default lives under `~/.cordial/<project>/`.

A reporter is optional. A lint that only needs to show up in a run's results can
pass an empty slice (`&[]`) in the hook table below.

## Bundling it up

The hooks make a check. To become an etiquette, the check also needs a name and
an explanation. First the hook tables, one slice per phase:

```rust
// Each hook is a static value of a unit struct, like `TodoRule` in Part 1.
static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static TODO_INVENTORY: TodoInventoryEnricher = TodoInventoryEnricher;
static TODO_PROBE: TodoSiteProbe = TodoSiteProbe;
static TODO_ASSESSOR: TodoAssessor = TodoAssessor;
static TODO_CSV: TodoCsvReporter = TodoCsvReporter;

// One slice per phase. Cordial's stock loader and enrichers sit alongside
// the ones we wrote.
static LOADERS: &[&dyn Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&dyn IrEnricher] = &[&SCOPE_ENRICHER, &TODO_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&dyn Probe] = &[&TODO_PROBE];
static ASSESSORS: &[&dyn Assessor] = &[&TODO_ASSESSOR];
static REPORTERS: &[&dyn Reporter] = &[&TODO_CSV];
```

Then the etiquette itself, which binds the tables to an id, a display name, and
an explain page:

```rust
pub static TODO_ETIQUETTE: LazyLock<StaticEtiquette> = LazyLock::new(|| {
    StaticEtiquette::new(
        // 1. Id and display name.
        "acme-todo",
        "Acme leftover todos",
        // 2. The hook tables. `None` is the workspace assessor slot.
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        // 3. Not a coverage etiquette.
        false,
        // 4. The explain page.
        EtiquetteExplain::new(
            // What the check is.
            "Leftover todo!() macros in source",
            // Why it exists.
            "Unfinished todo!() sites should not merge.",
            // What it does.
            "Walks the syntax tree for todo!() macros and emits ACME-TODO-001.",
            // How to opt out.
            "Do not register the acme-style plugin.",
            // The rule ids it can emit, with a note for each.
            vec![EtiquetteRuleExplain::new(
                "ACME-TODO-001",
                "Leftover `todo!()` macro",
            )],
        ),
    )
});
```

1. The id is what filters and reports use to refer to the etiquette.
2. The `None` is the workspace assessor slot, which this lint doesn't use. Any
   phase a lint doesn't need gets an empty slice (or `None`), so the table shows
   what is absent.
3. `false` says the etiquette is not a coverage check.
4. The explain page is what `cordial explain` prints, so it is what a person or
   an agent reads right after a finding. Its five fields are a one-line summary,
   why the check exists, what it does, how to opt out, and the rule ids with a
   note each. The [error-handling deep dive](error-handling-deep-dive.md) shows
   why the text matters: an agent in a loop has no other source for your house
   style.

Last, the plugin, which is the product you register:

```rust
pub static ACME_STYLE: LazyLock<StaticPlugin> = LazyLock::new(|| {
    // The plugin lives in a static and wants a `'static` slice of
    // etiquettes, so leak one small slice, once.
    let etiquettes: &'static [&'static dyn Etiquette] =
        Box::leak(Box::new([&*TODO_ETIQUETTE as &dyn Etiquette]));

    StaticPlugin::new(
        // The id a run filter will name, and a display name.
        "acme-style",
        "Acme style",
        // Quality plugins feed `quality-report.md`.
        PluginCategory::Quality,
        // The etiquettes this plugin contributes.
        etiquettes,
    )
});
```

A plugin is an id, a name, a category, and the etiquettes it contributes.

## What we have so far

Everything the lint needs is now written: a rule, an enricher, a probe, an
assessor, a reporter, an etiquette that binds them with an explain page, and a
plugin that cordial can register. What we haven't done is run any of it.

That is the whole of the next post.

**Next: [Part 5, Taking it for a run](writing-etiquettes-5-running.md).**

---

**The series: Your First Etiquette.**
[1. The plan and the rule](writing-etiquettes.md) ·
[2. Finding the sites](writing-etiquettes-2-finding-sites.md) ·
[3. Noticing and judging](writing-etiquettes-3-noticing-and-judging.md) ·
**4. Writing it down and bundling it up** ·
[5. Taking it for a run](writing-etiquettes-5-running.md) ·
[6. When order matters](writing-etiquettes-6-order.md) ·
[7. Make it yours](writing-etiquettes-7-make-it-yours.md) ·
[Reference](writing-etiquettes-reference.md)
