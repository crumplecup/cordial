# Your First Etiquette, Part 2: Finding the Sites

*Part 2 of 7. Previous: [The plan and the rule](writing-etiquettes.md). Next:
[Noticing and judging](writing-etiquettes-3-noticing-and-judging.md).*

Cordial splits a check into five jobs: load the files, add facts about them to a
shared graph, probe the graph for interesting spots, judge those spots, and
report. [Part 1](writing-etiquettes.md) laid out that plan for a `todo!()` lint
and named its rule, `ACME-TODO-001`. This post builds the second job, the
enricher. It finds every `todo!()` in a file and records each one in the
crate's graph, so the rest of cordial can see them.

The code comes in two pieces. The first is plain `syn`, the standard Rust
parsing crate, and knows nothing about cordial. The second connects it to
cordial.

*The numbered comments in the code match the notes after each block, as
described in [Part 1](writing-etiquettes.md#how-the-series-works).*

## Finding the sites

Before any cordial types come into it, the job is to parse a file, walk the
syntax tree, and notice macro calls named `todo`. The walk produces one record
per call:

```rust
// What the walk hands back for each `todo!()` it finds.
struct TodoRecord {
    context: String,
    file: std::path::PathBuf,
    line: u32,
    snippet: String,
}
```

The visitor that produces them carries what it needs while it walks the tree:

```rust
struct TodoVisitor {
    // Module path of the file, such as ["parser", "lexer"].
    module_prefix: Vec<String>,
    // The functions we are inside right now, innermost last.
    fn_stack: Vec<String>,
    // What the walk produces.
    records: Vec<TodoRecord>,
    // The file being walked.
    file: std::path::PathBuf,
}

impl TodoVisitor {
    // The module path followed by the function stack, such as
    // "parser::lexer::next_token", or "<crate>" when both are empty.
    fn context(&self) -> String {
        let mut parts = self.module_prefix.clone();
        parts.extend(self.fn_stack.iter().cloned());
        if parts.is_empty() {
            "<crate>".to_string()
        } else {
            parts.join("::")
        }
    }
}

impl<'ast> Visit<'ast> for TodoVisitor {
    // 1. Track the module we are in.
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.module_prefix.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.module_prefix.pop();
    }

    // 2. Track the function we are in: free functions, methods, and
    //    trait methods.
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, node);
        self.fn_stack.pop();
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_trait_item_fn(self, node);
        self.fn_stack.pop();
    }

    // 3. Look at every macro call.
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        // 4. Keep the ones named `todo`.
        if node.path.is_ident("todo") {
            // 5. Record where it is.
            self.records.push(TodoRecord {
                context: self.context(),
                file: self.file.clone(),
                line: node.span().start().line as u32,
                snippet: "todo!()".to_string(),
            });
        }
        // 6. Carry on with syn's default walk.
        syn::visit::visit_macro(self, node);
    }
}
```

1. An inline `mod` adds its name to the module path while its contents are
   walked. The path for the file itself is the starting value.
2. Each function's name goes on the stack before its body is walked and comes
   off afterward. Anything found inside sees the name, which is how a record can
   say `leftover` and not just a line number. Free functions, methods in `impl`
   blocks, and default methods in traits each have their own visitor method, so
   all three are covered.
3. `visit_macro` sees a macro call wherever it appears, whether it is used as an
   expression or written as a statement. `todo!("wire this up")` as the last
   expression of a function and `todo!();` with a semicolon both reach it.
4. Matching on the path is the whole test. `todo!(..)` is a macro whose path is
   `todo`.
5. The record's context comes from `context()`, shown above the visitor impl. It
   joins the module path and the function stack with `::`.
6. Calling syn's default method keeps the walk going.

## Giving the sites a home

The records need somewhere that other hooks can read them. That place is the
crate's graph: a `petgraph` of nodes and edges, one per crate, which enrichers
add to and probes read from. The enricher runs the visitor and writes each
record in as a node.

Each node gets a tag, so that a later hook can find exactly the nodes we add and
no others:

```rust
// The tag the enricher puts on every node it adds.
const TODO_ATTR: &str = "acme_todo";
```

Then the enricher itself:

```rust
impl IrEnricher for TodoInventoryEnricher {
    fn id(&self) -> &str {
        "acme-todo-inventory"
    }

    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        // 1. Split the view into its parts.
        let (ir, load, session) = view.into_parts();

        // 2. Skip anything that isn't a source load.
        let Some(source) = load.as_any().downcast_ref::<SourceLoadView>() else {
            return Ok(());
        };

        for file in source.files() {
            // 3. Parse the file.
            let syntax = syn::parse_file(file.source()).map_err(|err| {
                cordial::CordialError::syn_parse(file.path().display().to_string(), err)
            })?;

            // 4. Run the visitor.
            let module_prefix = cordial::module_path_from_src_file(source.src_root(), file.path());
            let mut visitor = TodoVisitor {
                module_prefix,
                fn_stack: Vec::new(),
                records: Vec::new(),
                file: file.path().clone(),
            };
            visitor.visit_file(&syntax);

            for record in visitor.records {
                // 5. Add a node for the record.
                let span = FileSpan::new(record.file.clone(), record.line, 1);
                let node = ir.insert_node(
                    NodeWeight::new(NodeKind::Expr)
                        .with_span(span)
                        .with_name(record.snippet.clone()),
                )?;

                // 6. Tag the node and store the facts the assessor will need.
                ir.set_attr(node, TODO_ATTR, serde_json::Value::Bool(true))?;
                ir.set_attr(node, "context", serde_json::Value::String(record.context))?;
                ir.set_attr(node, "snippet", serde_json::Value::String(record.snippet))?;
                ir.set_attr(
                    node,
                    "file",
                    // Relative to the project root, so reports don't depend
                    // on where the checkout lives.
                    serde_json::Value::String(
                        file.path()
                            .strip_prefix(session.project_root())
                            .unwrap_or(file.path())
                            .display()
                            .to_string(),
                    ),
                )?;
                ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;

                // 7. Attach the node to the crate root.
                ir.insert_edge(ir.root()?, node, EdgeKind::Contains)?;
            }
        }
        Ok(())
    }
}
```

1. The enricher receives one view and splits it into the graph to write into,
   the loader's output, and the session (which knows the project root).
2. The loader hands over the crate's files as a `SourceLoadView`. If the load is
   some other kind, there is nothing for this enricher to do.
3. A parse failure becomes a cordial error that names the file.
4. This is the visitor from the first half of this post, run over one file.
5. Each record becomes an expression node with a span and a readable name.
6. The `acme_todo = true` attribute is the tag. The others (`context`,
   `snippet`, `file`, `line`) are the facts the assessor reads back out.
7. Every node hangs off the crate root with a `Contains` edge.

Routing everything through a graph means nothing downstream has to know about
`syn`. The probe asks for nodes with a tag, and it gets them.

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
