# Your First Etiquette, Part 3: Noticing and Judging

*Part 3 of 7. Previous: [Finding the sites](writing-etiquettes-2-finding-sites.md).
Next: [Writing it down and bundling it up](writing-etiquettes-4-writing-and-bundling.md).*

The `todo!()` lint from [Part 1](writing-etiquettes.md) now has its first
working hook. [Part 2](writing-etiquettes-2-finding-sites.md) left every
`todo!()` in the crate as a node in the crate's graph, tagged `acme_todo` and
carrying four facts: the context, the snippet, the file, and the line. This post
covers the two hooks that turn those nodes into something a person can act on.
The probe notices them and wraps each in a *marker*, and the assessor decides
what each marker means and produces a *finding*.

*The numbered comments in the code match the notes after each block, as
described in [Part 1](writing-etiquettes.md#how-the-series-works).*

## Noticing them

A probe turns "there are tagged nodes" into markers. The question it asks is a
`Query`: which node kinds to look at, and a predicate for the final match.

```rust
// The label every marker from this probe will carry. The assessor
// subscribes to it. (TODO_ATTR is the tag from Part 2.)
const TODO_LABEL: &str = "acme-todo-site";

#[derive(Debug, Default, Clone, Copy)]
struct TodoSitesQuery;

static TODO_SITES_QUERY: TodoSitesQuery = TodoSitesQuery;

impl Query for TodoSitesQuery {
    // A cheap prefilter: only expression nodes.
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    // This query doesn't follow edges.
    fn edge_kinds(&self) -> &[EdgeKind] {
        &[]
    }

    // The actual test: does the node carry our tag?
    fn matches_node(&self, node: &dyn NodeView) -> bool {
        node.attr(TODO_ATTR).is_some()
    }
}
```

The probe runs the query and wraps each hit in a marker:

```rust
impl Probe for TodoSiteProbe {
    fn id(&self) -> &str {
        "acme-todo-site"
    }

    // 1. Say which nodes this probe cares about.
    fn interests(&self) -> &dyn Query {
        &TODO_SITES_QUERY
    }

    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir();

        Ok(ir
            // 2. Run the query over the graph.
            .nodes_matching(&TODO_SITES_QUERY)
            .into_iter()
            .map(|node| {
                // 3. Wrap each hit in a marker.
                Box::new(TodoMarker {
                    anchor: NodeAnchor::new(node.id()),
                }) as Box<dyn Marker>
            })
            .collect())
    }
}
```

1. `interests` hands cordial the query, so it knows which nodes this probe
   looks at.
2. `nodes_matching` runs the query and returns every node that passes.
3. Each hit becomes a `TodoMarker`, shown next.

A marker is a pointer back at a graph node, plus a label:

```rust
#[derive(Debug, Clone)]
struct TodoMarker {
    anchor: NodeAnchor,
}

impl Marker for TodoMarker {
    // Both of these report the label from above.
    fn probe(&self) -> &str {
        TODO_LABEL
    }

    fn label(&self) -> &str {
        TODO_LABEL
    }

    // The graph node this marker points at.
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    // No source span of its own. The node it points at has one.
    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}
```

A marker says only "this site exists." It carries no opinion, which is why a
probe never has to know what counts as a violation.

## Judging them

The assessor receives the markers carrying the labels it asked for, and decides
what each one means. This lint's rule is simple: every marker is a violation.

```rust
impl Assessor for TodoAssessor {
    fn id(&self) -> &str {
        "acme-todo-assessor"
    }

    // 1. Subscribe to the probe's label.
    fn consumes(&self) -> Vec<&str> {
        vec![TODO_LABEL]
    }

    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        let markers = view.markers();
        let ir = view.ir();
        let session = view.session();

        let mut findings = Vec::new();
        for marker in markers {
            // 2. Follow the marker back to its node.
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };

            // 3. Read back what the enricher stored.
            let context = node
                .attr("context")
                .and_then(|v| v.as_str())
                .unwrap_or("<crate>")
                .to_string();
            let snippet = node
                .attr("snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("todo!()")
                .to_string();
            let line = node.attr("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let file = node
                .attr("file")
                .and_then(|v| v.as_str())
                .map(|path| session.project_root().join(path))
                .unwrap_or_else(|| session.project_root().to_path_buf());

            // 4. Judge it.
            findings.push(Box::new(TodoFinding {
                rule: TodoRule,
                disposition: Disposition::Open,
                anchor: NodeAnchor::new(node_id),
                crate_name: ir.crate_name().to_string(),
                context,
                span: FileSpan::new(file, line, 1),
                snippet,
            }) as Box<dyn Finding>);
        }
        Ok(findings)
    }
}
```

1. `consumes` is the subscription. Only markers with this label reach `assess`.
2. Each marker's anchor leads back to the node it came from.
3. Most of the body is reading attributes back out of the node, with a fallback
   for each in case one is missing.
4. The line that carries the judgment is `disposition: Disposition::Open`. A
   finding is always in one of three states: `Open` (still a problem),
   `Exemplar` (kept on purpose as a good example, not a defect), or
   `Suppressed` (silenced by an exception). Assessors decide which. This lint
   has no exceptions, so every finding is open.

The finding itself is a small struct that points at its rule and anchor, and
writes its fields out for reporters:

```rust
#[derive(Debug, Clone)]
struct TodoFinding {
    rule: TodoRule,
    disposition: Disposition,
    anchor: NodeAnchor,
    crate_name: String,
    context: String,
    span: FileSpan,
    snippet: String,
}

impl Finding for TodoFinding {
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    fn disposition(&self) -> Disposition {
        self.disposition
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    // Reporters see a finding only through this method, so these named
    // fields are the format they can rely on.
    fn emit(&self, sink: &mut dyn FindingSink) {
        sink.field("crate", &self.crate_name);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("snippet", &self.snippet);
        // The excerpt shown with the finding.
        sink.snippet(&self.snippet);
    }
}
```

## What we have so far

For each `todo!()` in the crate, the lint now produces an open finding that
points at `ACME-TODO-001` and carries the crate, context, file, line, and
snippet. The `emit` method is the contract those fields travel through, and the
next hook reads them back by name.

What is still missing is somewhere for the findings to go, and a way to register
the lint with cordial at all. Both are in the next post.

**Next: [Part 4, Writing it down and bundling it up](writing-etiquettes-4-writing-and-bundling.md).**

---

**The series: Your First Etiquette.**
[1. The plan and the rule](writing-etiquettes.md) ·
[2. Finding the sites](writing-etiquettes-2-finding-sites.md) ·
**3. Noticing and judging** ·
[4. Writing it down and bundling it up](writing-etiquettes-4-writing-and-bundling.md) ·
[5. Taking it for a run](writing-etiquettes-5-running.md) ·
[6. When order matters](writing-etiquettes-6-order.md) ·
[7. Make it yours](writing-etiquettes-7-make-it-yours.md) ·
[Reference](writing-etiquettes-reference.md)
