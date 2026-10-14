# Your First Etiquette, Part 5: Taking It for a Run

*Part 5 of 7. Previous: [Writing it down and bundling it up](writing-etiquettes-4-writing-and-bundling.md).
Next: [When order matters](writing-etiquettes-6-order.md).*

Four posts in, the lint is fully written and has never run. In
[Part 1](writing-etiquettes.md) we set the bar: pointed at a crate whose
`src/lib.rs` has a `todo!()` on line 2 of a function named `leftover`, the lint
should produce one finding, with rule id `ACME-TODO-001`, at `src/lib.rs`, line
2, in function `leftover`. This post checks that, and then shows what the real
output looks like.

A plugin you write runs through a `Session`, the object that drives a cordial
run, which you build in code. The `cordial` command runs the built-in
etiquettes. A test is the natural place to build a session for the first run.

*The numbered comments in the code match the notes after each block, as
described in [Part 1](writing-etiquettes.md#how-the-series-works).*

## Taking it for a run

Cordial's repository keeps tests under `tests/`, so that is where this one goes.
The test needs the plugin in scope. The repo's own test pulls the example module in by
path, and if your plugin lives in the same crate a plain `use` does the same
job:

```rust
// The plugin module from the cordial repository's example.
#[path = "../examples/custom_plugins/quality.rs"]
mod quality;

use cordial::{NamedRunFilter, SessionBuilder};
use miette::IntoDiagnostic;
use quality::ACME_STYLE;
```

Then plant a `todo!()` in a scratch crate and check what comes back:

```rust
#[test]
fn flags_a_leftover_todo() -> miette::Result<()> {
    // 1. Build a scratch crate with one `todo!()` on line 2.
    let project = tempfile::tempdir().into_diagnostic()?;
    std::fs::create_dir_all(project.path().join("src")).into_diagnostic()?;
    std::fs::write(
        project.path().join("src/lib.rs"),
        "pub fn leftover() {\n    todo!(\"wire this up\")\n}\n",
    )
    .into_diagnostic()?;

    // 2. Build a session with a throwaway store and our plugin.
    let store = tempfile::tempdir().into_diagnostic()?;
    let session = SessionBuilder::new(project.path())
        .with_store_root(store.path())
        .register_plugin(&*ACME_STYLE)
        .build();

    // 3. Run only that plugin.
    let outcome = session
        .run(&NamedRunFilter::plugins(["acme-style"]))
        .into_diagnostic()?;

    // 4. Check the result against the contract.
    let findings: Vec<_> = outcome.findings().collect();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule().id(), "ACME-TODO-001");

    let mut sink = cordial::MapFindingSink::default();
    findings[0].emit(&mut sink);
    let field = |name: &str| {
        sink.fields()
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    assert_eq!(field("line"), "2");
    assert_eq!(field("context"), "leftover");
    assert!(field("file").ends_with("src/lib.rs"));
    Ok(())
}
```

1. The scratch crate is the smallest input that exercises the lint.
2. The session gets its own store, so the artifacts land somewhere disposable.
   Plugins are registered whole, never as loose hooks.
3. A `NamedRunFilter` narrows the run to the plugin you care about.
4. One planted `todo!()` should produce one finding with our rule id, and its
   fields should match the contract. The fields are read back with the same
   by-name lookup the reporter uses.

The repo's own test in `tests/custom_plugins.rs` has the same shape.

## Seeing the output

A passing assertion is reassuring, but the real product is the report. To see
it, point the session at your own project and a store you keep, instead of the
throwaway one. This test is marked `#[ignore]`, so it runs only when asked for:

```rust
#[test]
#[ignore = "run by hand against a real project"]
fn acme_todo_report() -> miette::Result<()> {
    let session = SessionBuilder::new("/path/to/your/project")
        .with_store_root(std::path::Path::new("target/acme-store"))
        .register_plugin(&*ACME_STYLE)
        .build();

    let _outcome = session
        .run(&NamedRunFilter::plugins(["acme-style"]))
        .into_diagnostic()?;
    Ok(())
}
```

Run it, then look in the store for the artifact the reporter wrote:

```sh
cargo test acme_todo_report -- --ignored
find target/acme-store -name acme-todo.csv
```

The file is the CSV from the reporter, with one row per `todo!()`. For a project
with one leftover in a method, it looks like this:

```csv
crate,context,file,line,snippet
my_app,parser::lexer::next_token,/home/you/my_app/src/parser/lexer.rs,41,todo!()
```

The context reads as the module path followed by the function: `next_token`,
in `src/parser/lexer.rs`. That is the column to scan when deciding whether a
leftover is in code that is about to change anyway.

## What we have so far

A working lint. It finds every `todo!()`, judges each one, and writes a CSV you
can open. Everything past this point is about making it fit better, not making
it work.

The next post is about ordering, and it is optional. If you only want to see how
to vary the lint or add it to the quality report, skip to Part 7.

**Next: [Part 6, When order matters](writing-etiquettes-6-order.md).** Or skip to
[Part 7, Make it yours](writing-etiquettes-7-make-it-yours.md).

---

**The series: Your First Etiquette.**
[1. The plan and the rule](writing-etiquettes.md) ·
[2. Finding the sites](writing-etiquettes-2-finding-sites.md) ·
[3. Noticing and judging](writing-etiquettes-3-noticing-and-judging.md) ·
[4. Writing it down and bundling it up](writing-etiquettes-4-writing-and-bundling.md) ·
**5. Taking it for a run** ·
[6. When order matters](writing-etiquettes-6-order.md) ·
[7. Make it yours](writing-etiquettes-7-make-it-yours.md) ·
[Reference](writing-etiquettes-reference.md)
