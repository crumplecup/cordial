//! Plugin-only family: a [`StaticPlugin`] wrapping one custom etiquette.
//!
//! This is the quality-family template. There is no extra supertrait — the
//! plugin is just an id, a category, and the etiquettes it contributes.

use cordial::{
    AssessView, Assessor, AttributeEnricher, CordialResult, Disposition, EdgeKind, Etiquette,
    EtiquetteExplain, EtiquetteHooks, EtiquetteRuleExplain, FileSpan, Finding, FindingSink,
    IrAnchor, IrEnricher, Loader, Marker, NodeAnchor, NodeKind, NodeView, NodeWeight,
    PluginCategory, Probe, ProbeView, Query, RenderView, Reporter, Rule, ScopeEnricher,
    SourceEnrichView, SourceEnricher, SourceLoader, SourceSpan, StaticEtiquette, StaticPlugin,
    TextArtifact,
};
use std::sync::LazyLock;

use cordial::SourceSyntax;
use cordial::syntax::AstNode;
use cordial::syntax::ast::{self, HasName};

use tracing::instrument;
static SOURCE_LOADER: SourceLoader = SourceLoader;
static SCOPE_ENRICHER: ScopeEnricher = ScopeEnricher;
static ATTRIBUTE_ENRICHER: AttributeEnricher = AttributeEnricher;
static TODO_INVENTORY: TodoInventoryEnricher = TodoInventoryEnricher;
static TODO_PROBE: TodoSiteProbe = TodoSiteProbe;
static TODO_ASSESSOR: TodoAssessor = TodoAssessor;
static TODO_CSV: TodoCsvReporter = TodoCsvReporter;

static LOADERS: &[&dyn Loader] = &[&SOURCE_LOADER];
static ENRICHERS: &[&dyn IrEnricher] = &[&SCOPE_ENRICHER, &TODO_INVENTORY, &ATTRIBUTE_ENRICHER];
static PROBES: &[&dyn Probe] = &[&TODO_PROBE];
static ASSESSORS: &[&dyn Assessor] = &[&TODO_ASSESSOR];
static REPORTERS: &[&dyn Reporter] = &[&TODO_CSV];

/// Flags leftover `todo!()` macros in source.
pub static TODO_ETIQUETTE: LazyLock<StaticEtiquette> = LazyLock::new(|| {
    StaticEtiquette::new(
        "acme-todo",
        "Acme leftover todos",
        EtiquetteHooks::new(LOADERS, ENRICHERS, PROBES, ASSESSORS, None, REPORTERS),
        false,
        EtiquetteExplain::new(
            "Leftover todo!() macros in source",
            "Unfinished todo!() sites should not merge. This example shows a quality-family plugin wrapping one etiquette.",
            "Walks the shared rust-analyzer tree for todo!() macros and emits ACME-TODO-001. Not a built-in; copy examples/custom_plugins.",
            "Set [lints.acme-todo] enabled = false in cordial.toml. Individual rules and paths use the same shared controls as built-ins.",
            vec![EtiquetteRuleExplain::new(
                "ACME-TODO-001",
                "Leftover `todo!()` macro",
            )],
        ),
    )
});

/// Acme style family — Plugin only, no Coverage / ErrorHandling supertrait.
///
/// `StaticPlugin::new` still needs a `&'static [&'static dyn Etiquette]` —
/// a crate-local trait-object table, the same accepted shape
/// `EtiquetteHooks` uses — so this leaks that one small slice once.
pub static ACME_STYLE: LazyLock<StaticPlugin> = LazyLock::new(|| {
    let etiquettes: &'static [&'static dyn Etiquette] =
        Box::leak(Box::new([&*TODO_ETIQUETTE as &dyn Etiquette]));
    StaticPlugin::new(
        "acme-style",
        "Acme style",
        PluginCategory::Quality,
        etiquettes,
    )
});

const TODO_ATTR: &str = "acme_todo";
const TODO_LABEL: &str = "acme-todo-site";

#[derive(Debug, Clone, Copy)]
struct TodoRule;

impl cordial::TomlConfigured for TodoRule {
    #[tracing::instrument(level = "trace", skip(self))]
    fn configuration_key(&self) -> &str {
        "acme-todo"
    }
}

impl Rule for TodoRule {
    #[tracing::instrument(level = "trace", skip(self))]
    fn category(&self) -> &str {
        "acme_style"
    }

    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "ACME-TODO-001"
    }

    #[instrument(level = "trace", skip(self))]
    fn description(&self) -> &str {
        "Leftover `todo!()` macro — finish or delete before merge"
    }
}

#[derive(Debug, Clone)]
struct TodoMarker {
    anchor: NodeAnchor,
}

impl Marker for TodoMarker {
    #[instrument(level = "trace", skip(self))]
    fn probe(&self) -> &str {
        TODO_LABEL
    }

    #[instrument(level = "trace", skip(self))]
    fn label(&self) -> &str {
        TODO_LABEL
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    #[instrument(level = "trace", skip(self))]
    fn span(&self) -> Option<&dyn SourceSpan> {
        None
    }
}

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
    #[instrument(level = "trace", skip(self))]
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    #[instrument(level = "trace", skip(self))]
    fn disposition(&self) -> Disposition {
        self.disposition
    }

    #[instrument(level = "trace", skip(self))]
    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    #[instrument(level = "trace", skip(self, sink))]
    fn emit(&self, sink: &mut dyn FindingSink) {
        sink.field("crate", &self.crate_name);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file().display().to_string());
        sink.field("line", &self.span.line().to_string());
        sink.field("snippet", &self.snippet);
        sink.snippet(&self.snippet);
    }
}

#[derive(Debug, Clone)]
struct TodoRecord {
    context: String,
    line: u32,
    snippet: String,
}

#[derive(Debug, Default, Clone, Copy)]
struct TodoInventoryEnricher;

impl SourceEnricher for TodoInventoryEnricher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "acme-todo-inventory"
    }

    #[instrument(level = "trace", skip(self, view))]
    fn enrich_source(&self, view: SourceEnrichView<'_>) -> CordialResult<()> {
        let (ir, source, session) = view.into_parts();

        for file in source.files() {
            let module_prefix = cordial::module_path_from_src_file(source.src_root(), file.path());
            // The database owns the tree; this callback borrows it.
            let (path, records) = source.with_syntax(file.path(), |syntax| {
                Ok((
                    syntax.path().to_path_buf(),
                    todo_sites(&syntax, &module_prefix),
                ))
            })?;
            for record in records {
                let span = FileSpan::new(path.clone(), record.line, 1);
                let node = ir.insert_node(
                    NodeWeight::new(NodeKind::Expr)
                        .with_span(span)
                        .with_name(record.snippet.clone()),
                )?;
                ir.set_attr(node, TODO_ATTR, serde_json::Value::Bool(true))?;
                ir.set_attr(node, "context", serde_json::Value::String(record.context))?;
                ir.set_attr(node, "snippet", serde_json::Value::String(record.snippet))?;
                ir.set_attr(
                    node,
                    "file",
                    serde_json::Value::String(
                        path.strip_prefix(session.project_root())
                            .unwrap_or(&path)
                            .display()
                            .to_string(),
                    ),
                )?;
                ir.set_attr(node, "line", serde_json::Value::Number(record.line.into()))?;
                ir.insert_edge(ir.root()?, node, EdgeKind::Contains)?;
            }
        }
        Ok(())
    }
}

/// Every `todo!()` call in the file, with the functions enclosing it. The tree
/// holds code under `#[cfg]` too, so those sites are found as well.
#[instrument(level = "debug", skip(file))]
fn todo_sites(file: &SourceSyntax<'_>, module_prefix: &[String]) -> Vec<TodoRecord> {
    file.syntax()
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|call| {
            call.path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .is_some_and(|name| name.text() == "todo")
        })
        .map(|call| {
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
                line: file.line(call.syntax().text_range().start()),
                snippet: "todo!()".to_string(),
            }
        })
        .collect()
}

#[derive(Debug, Default, Clone, Copy)]
struct TodoSitesQuery;

impl Query for TodoSitesQuery {
    #[instrument(level = "trace", skip(self))]
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    #[instrument(level = "trace", skip(self))]
    fn edge_kinds(&self) -> &[cordial::EdgeKind] {
        &[]
    }

    #[instrument(level = "trace", skip(self, node))]
    fn matches_node(&self, node: &dyn NodeView) -> bool {
        node.attr(TODO_ATTR).is_some()
    }
}

static TODO_SITES_QUERY: TodoSitesQuery = TodoSitesQuery;

#[derive(Debug, Default, Clone, Copy)]
struct TodoSiteProbe;

impl Probe for TodoSiteProbe {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "acme-todo-site"
    }

    #[instrument(level = "trace", skip(self, view))]
    fn probe(&self, view: ProbeView<'_>) -> CordialResult<Vec<Box<dyn Marker>>> {
        let ir = view.ir();

        Ok(ir
            .nodes_matching(&TODO_SITES_QUERY)
            .into_iter()
            .map(|node| {
                Box::new(TodoMarker {
                    anchor: NodeAnchor::new(node.id()),
                }) as Box<dyn Marker>
            })
            .collect())
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct TodoAssessor;

impl Assessor for TodoAssessor {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "acme-todo-assessor"
    }

    #[instrument(level = "trace", skip(self))]
    fn consumes(&self) -> Vec<&str> {
        vec![TODO_LABEL]
    }

    #[instrument(level = "trace", skip(self, view))]
    fn assess(&self, view: AssessView<'_>) -> CordialResult<Vec<Box<dyn Finding>>> {
        let markers = view.markers();
        let ir = view.ir();
        let session = view.session();

        let mut findings = Vec::new();
        for marker in markers {
            let node_id = marker.anchor().node_id();
            let Some(node) = ir.node(node_id) else {
                continue;
            };
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

#[derive(Debug, Default, Clone, Copy)]
struct TodoCsvReporter;

impl Reporter for TodoCsvReporter {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> &str {
        "acme-todo-csv"
    }

    #[instrument(level = "trace", skip(self, view))]
    fn render(&self, view: RenderView<'_>) -> CordialResult<Vec<Box<dyn cordial::Artifact>>> {
        let findings = view.findings();

        let mut body = String::from("crate,context,file,line,snippet\n");
        for finding in findings {
            if finding.rule().id() != "ACME-TODO-001" {
                continue;
            }
            let mut sink = cordial::MapFindingSink::default();
            finding.emit(&mut sink);
            let field = |name: &str| {
                sink.fields()
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.clone())
                    .unwrap_or_default()
            };
            body.push_str(&format!(
                "{},{},{},{},{}\n",
                field("crate"),
                field("context"),
                field("file"),
                field("line"),
                field("snippet"),
            ));
        }
        Ok(vec![Box::new(TextArtifact::new(
            "acme-todo.csv".to_string(),
            "text/csv".to_string(),
            body,
        ))])
    }
}
