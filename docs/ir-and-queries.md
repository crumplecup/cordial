# IR and queries

The graph IR is the shared workspace model behind cordial runs. Loaders build
it, enrichers add facts, probes query it, assessors read it while judging
markers, and reporters read it while rendering artifacts.

Use this guide when writing custom probes or enrichers. For the full pipeline,
read [Writing etiquettes](writing-etiquettes.md).

## Mental model

| Piece | Meaning | Common user |
| --- | --- | --- |
| `WorkspaceIr` | all crate IRs in one run | workspace assessors |
| `CrateIr` | one crate's graph | loaders and tests |
| `IrView` | read-only crate view | probes, assessors, reporters |
| `IrMut` | mutable crate view | enrichers |
| `NodeWeight` | node kind, name, span, attributes | loaders and enrichers |
| `EdgeWeight` | directed graph relationship | loaders and enrichers |
| `Query` | reusable node predicate | probes |

Node ids are local to one crate graph. Treat rule ids, marker labels, artifact
names, and source spans as the stable replay surface; do not treat `NodeId` as a
cross-run identifier.

## Reading nodes

Most probes start from `IrView::nodes_matching`:

```rust,ignore
use cordial::{BasicQuery, CordialResult, Marker, NodeKind, Probe, ProbeView};

struct PublicTypeProbe;

impl Probe for PublicTypeProbe {
    fn id(&self) -> &str {
        "acme-public-type"
    }

    fn interests(&self) -> &dyn cordial::Query {
        static QUERY: BasicQuery = BasicQuery::ALL_NODES;
        &QUERY
    }

    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let query = BasicQuery::all_nodes();
        let public_types = view.ir.nodes_matching(&query).into_iter().filter(|node| {
            matches!(
                node.kind(),
                NodeKind::Item(cordial::ItemKind::Struct)
                    | NodeKind::Item(cordial::ItemKind::Enum)
                    | NodeKind::Item(cordial::ItemKind::Trait)
            ) && node
                .attr("is_public")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        });

        Ok(public_types.map(|node| marker_for(node.id)).collect())
    }
}
```

Use `node_by_path` when you have a fully qualified path such as
`crate_name::module::Type`. Use `parents` and `children` when the rule depends
on topology, not just node attributes.

## Built query predicates

`QueryBuilder` covers the common "kind plus attribute" case:

```rust,ignore
use cordial::{ItemKind, NodeKind, QueryBuilder};

let query = QueryBuilder::new()
    .node_kinds([NodeKind::Item(ItemKind::Fn)])
    .has_attr("tracing_role")
    .build();
```

Use `with_attr` when the attribute is a string and the rule needs an exact
value:

```rust,ignore
let query = QueryBuilder::new()
    .node_kinds([NodeKind::Expr])
    .with_attr("panic_kind", "explicit")
    .build();
```

Use a custom `Query` when the predicate needs typed JSON, multiple attributes,
or richer fallback behavior:

```rust,ignore
use cordial::{EdgeKind, NodeKind, NodeView, Query};

struct TodoSitesQuery;

impl Query for TodoSitesQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    fn edge_kinds(&self) -> &[EdgeKind] {
        &[]
    }

    fn matches_node(&self, node: &dyn NodeView) -> bool {
        node.attr("acme_todo").is_some()
    }
}
```

The custom plugin example uses this exact shape in
`examples/custom_plugins/quality.rs`.

## Adding facts

Enrichers add facts through `IrMut`. Prefer attributes for scanner facts and
edges for relationships later rules may traverse.

```rust,ignore
use cordial::{CordialResult, EdgeKind, EnrichView, IrEnricher, NodeKind, NodeWeight};

struct TodoInventoryEnricher;

impl IrEnricher for TodoInventoryEnricher {
    fn id(&self) -> &str {
        "acme-todo-inventory"
    }

    fn enrich(&self, view: EnrichView<'_>) -> CordialResult<()> {
        let node = view.ir.insert_node(NodeWeight::new(NodeKind::Expr))?;
        view.ir.set_attr(node, "acme_todo", serde_json::Value::Bool(true))?;
        view.ir.insert_edge(view.ir.root()?, node, EdgeKind::Contains)?;
        Ok(())
    }
}
```

If an enricher adds or changes `qualified_path` attributes, call
`IrMut::rebuild_path_index` before later hooks rely on `node_by_path`.

## Attribute conventions

Built-in loaders and enrichers use JSON attributes rather than bespoke node
types. Important keys include:

| Attribute | Meaning |
| --- | --- |
| `qualified_path` | path index key used by `node_by_path` |
| `ir_origin` | whether a fact came from source, rustdoc, or a peer link |
| `rustdoc_kind` | rustdoc item kind |
| `is_public` | public API visibility fact |
| `trait_impls` | trait implementation inventory attached to a type |
| `wraps_foreign` | wrapper relationship over a foreign type |

Custom plugins should namespace their attributes, for example `acme_todo`, when
the key is not meant to be shared across plugins.
