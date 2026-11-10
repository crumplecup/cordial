# Your First Etiquette, Part 2: Finding the Sites

*Part 2 of 7. Previous: [The plan and the rule](writing-etiquettes.md). Next:
[Noticing and judging](writing-etiquettes-3-noticing-and-judging.md).*

Cordial splits a check into five jobs: load the files, add facts about them to a
shared graph, probe the graph for interesting spots, judge those spots, and
report. [Part 1](writing-etiquettes.md) laid out that plan for a `todo!()` lint
and named its rule, `ACME-TODO-001`. This post builds the second job, the
enricher. It finds every `todo!()` in a file and records each one in the
crate's graph, so the rest of cordial can see them.

The code comes in two pieces. The first is a walk over the syntax tree and
knows nothing about the graph. The second connects it to cordial.

*The numbered comments in the code match the notes after each block, as
described in [Part 1](writing-etiquettes.md#how-the-series-works).*

## Finding the sites

Before the graph comes into it, the job is to walk a parsed file and notice
macro calls named `todo`. The walk produces one record per call:

```rust
// What the walk hands back for each `todo!()` it finds.
struct TodoRecord {
    context: String,
    line: u32,
    snippet: String,
}
```

The tree is rust-analyzer's, the same one cordial's own checks read. Cordial
parses each file once, in the run's database, and lends the tree to your hook
as a `SourceSyntax`. Import rust-analyzer's types from `cordial::syntax`, so
that your plugin and cordial agree on one version of them:

```rust
use cordial::SourceSyntax;
use cordial::syntax::AstNode;
use cordial::syntax::ast::{self, HasName};

// Every `todo!()` call in the file, with the functions around it.
fn todo_sites(file: &SourceSyntax<'_>, module_prefix: &[String]) -> Vec<TodoRecord> {
    file.syntax()
        .syntax()
        .descendants()                                      // 1
        .filter_map(ast::MacroCall::cast)                   // 2
        .filter(|call| {                                    // 3
            call.path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .is_some_and(|name| name.text() == "todo")
        })
        .map(|call| {
            // 4. The enclosing functions, outermost first.
            let mut parts = module_prefix.to_vec();
            let mut functions: Vec<String> = call
                .syntax()
                .ancestors()
                .filter_map(ast::Fn::cast)
                .filter_map(|function| function.name())
                .map(|name| name.text().to_string())
                .collect();
            functions.reverse();
            parts.extend(functions);
            TodoRecord {
                context: if parts.is_empty() {
                    "<crate>".to_string()
                } else {
                    parts.join("::")
                },
                // 5. Lines are counted from one.
                line: file.line(call.syntax().text_range().start()),
                snippet: "todo!()".to_string(),
            }
        })
        .collect()
}
```

1. `descendants` visits every node under the file, depth first. There is no
   visitor to write: the tree already holds every item, including code under
   `#[cfg(..)]`, which is how a `todo!()` in a gated function is found too.
2. `MacroCall::cast` keeps the macro calls, whether they are used as an
   expression or written as a statement. `todo!("wire this up")` as the last
   expression of a function and `todo!();` with a semicolon both reach it.
3. Matching on the path's last segment is the whole test. `todo!(..)` is a
   macro whose path is `todo`.
4. Walking up through `ancestors` collects every enclosing function: free
   functions, methods in `impl` blocks and default methods in traits are all
   `ast::Fn`. That is how a record can say `leftover` and not just a line
   number. The module path of the file is the starting value.
5. `SourceSyntax::line` turns a byte offset into a line number, using the same
   line table cordial's own records use.

`file.syntax()` borrows the tree. `file.into_inner()` hands you the tree itself
if you want to pass it to your own code; keep `file.snapshot()` (the path and
digest of the text it came from) with any facts you derive from it.

## Giving the sites a home

The records need somewhere that other hooks can read them. That place is the
crate's graph: a `petgraph` of nodes and edges, one per crate, which enrichers
add to and probes read from. The enricher runs the walk and writes each
record in as a node.

Each node gets a tag, so that a later hook can find exactly the nodes we add and
no others:

```rust
// The tag the enricher puts on every node it adds.
const TODO_ATTR: &str = "acme_todo";
```

Then the enricher itself:

```rust
impl SourceEnricher for TodoInventoryEnricher {
    fn id(&self) -> &str {
        "acme-todo-inventory"
    }

    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        // 1. The typed view gives us the graph, source loader, and session.
        let (ir, source, session) = view.into_parts();

        for file in source.files() {
            let module_prefix =
                cordial::module_path_from_src_file(source.src_root(), file.path());

            // 2. Borrow the shared tree for this file. The database owns it,
            //    so the handle cannot leave the closure.
            let (path, records) = source.with_syntax(file.path(), |syntax| {
                // 3. Run the walk from the first half of this post.
                Ok((syntax.path().to_path_buf(), todo_sites(&syntax, &module_prefix)))
            })?;

            for record in records {
                // 4. Add a node using the path paired with the parsed tree.
                let span = FileSpan::new(path.clone(), record.line, 1);
                let node = ir.insert_node(
                    NodeWeight::new(NodeKind::Expr)
                        .with_span(span)
                        .with_name(record.snippet.clone()),
                )?;

                // 5. Tag the node and store the facts the assessor will need.
                ir.set_attr(node, TODO_ATTR, serde_json::Value::Bool(true))?;
                ir.set_attr(node, "context", serde_json::Value::String(record.context))?;
                ir.set_attr(node, "snippet", serde_json::Value::String(record.snippet))?;
                ir.set_attr(
                    node,
                    "file",
                    // Relative to the project root, so reports don't depend
                    // on where the checkout lives.
                    serde_json::Value::String(
                        path.strip_prefix(session.project_root())
                            .unwrap_or(&path)
                            .display()
                            .to_string(),
                    ),
                )?;
                ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;

                // 6. Attach the node to the crate root.
                ir.insert_edge(ir.root()?, node, EdgeKind::Contains)?;
            }
        }
        Ok(())
    }
}
```

1. The source-specific hook receives a typed view containing the graph, the
   source loader, and the session (which knows the project root).
2. `with_syntax` lends the file's tree from the run's database, which parses
   each file once for every hook. The handle keeps the path, text and tree
   together, and parse errors identify the file.
3. This is the walk from the first half of this post, run over that tree.
4. Each record becomes an expression node with a span from the same file.
5. The `acme_todo = true` attribute is the tag. The others (`context`,
   `snippet`, `file`, `line`) are the facts the assessor reads back out.
6. Every node hangs off the crate root with a `Contains` edge.

Routing everything through a graph means nothing downstream has to know about
syntax trees. The probe asks for nodes with a tag, and it gets them.

Cordial's two stock enrichers sit alongside ours when we assemble the hook
tables in Part 4. `ScopeEnricher` adds scope edges from items to their enclosing
module, and `AttributeEnricher` adds nodes for attributes.

For more on the graph itself, read [IR and queries](ir-and-queries.md).

## What we have so far

After this hook runs, every `todo!()` in the crate is a node in the graph. Each
carries the `acme_todo` tag and four facts: the context, the snippet, the file,
and the line. Nothing has said yet whether any of them is a problem. That is
deliberate, and it is the next post's job.

Next we ask the graph for those nodes, wrap each in a marker, and judge them.

**Next: [Part 3, Noticing and judging](writing-etiquettes-3-noticing-and-judging.md).**

---

**The series: Your First Etiquette.**
[1. The plan and the rule](writing-etiquettes.md) ·
**2. Finding the sites** ·
[3. Noticing and judging](writing-etiquettes-3-noticing-and-judging.md) ·
[4. Writing it down and bundling it up](writing-etiquettes-4-writing-and-bundling.md) ·
[5. Taking it for a run](writing-etiquettes-5-running.md) ·
[6. When order matters](writing-etiquettes-6-order.md) ·
[7. Make it yours](writing-etiquettes-7-make-it-yours.md) ·
[Reference](writing-etiquettes-reference.md)
