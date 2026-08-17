//! Plugin-only family: a [`StaticPlugin`] wrapping one custom etiquette.
//!
//! This is the quality-family template. There is no extra supertrait — the
//! plugin is just an id, a category, and the etiquettes it contributes.

use cordial::{
    Assessor, AttributeEnricher, CordialResult, Disposition, EdgeKind, Etiquette, FileSpan,
    Finding, FindingSink, IrAnchor, IrEnricher, IrMut, IrView, LoadView, Loader, Marker,
    NodeAnchor, NodeKind, NodeView, NodeWeight, PluginCategory, Probe, Query, Reporter, Rule,
    ScopeEnricher, SessionView, SourceLoadView, SourceLoader, SourceSpan, StaticEtiquette,
    StaticPlugin, TextArtifact,
};
use syn::spanned::Spanned;
use syn::visit::Visit;

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
pub static TODO_ETIQUETTE: StaticEtiquette = StaticEtiquette {
    id: "acme-todo",
    name: "Acme leftover todos",
    loaders: LOADERS,
    enrichers: ENRICHERS,
    probes: PROBES,
    assessors: ASSESSORS,
    workspace_assessors: None,
    reporters: REPORTERS,
    is_coverage: false,
};

static ACME_STYLE_ETIQUETTES: &[&dyn Etiquette] = &[&TODO_ETIQUETTE];

/// Acme style family — Plugin only, no Coverage / ErrorHandling supertrait.
pub static ACME_STYLE: StaticPlugin = StaticPlugin {
    id: "acme-style",
    name: "Acme style",
    category: PluginCategory::Quality,
    etiquettes: ACME_STYLE_ETIQUETTES,
};

const TODO_ATTR: &str = "acme_todo";
const TODO_LABEL: &str = "acme-todo-site";

#[derive(Debug, Clone, Copy)]
struct TodoRule;

impl Rule for TodoRule {
    fn id(&self) -> &str {
        "ACME-TODO-001"
    }

    fn category(&self) -> &str {
        "acme_style"
    }

    fn description(&self) -> &str {
        "Leftover `todo!()` macro — finish or delete before merge"
    }
}

#[derive(Debug, Clone)]
struct TodoMarker {
    anchor: NodeAnchor,
}

impl Marker for TodoMarker {
    fn probe(&self) -> &str {
        TODO_LABEL
    }

    fn label(&self) -> &str {
        TODO_LABEL
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

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
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    fn disposition(&self) -> Disposition {
        self.disposition
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn emit(&self, sink: &mut dyn FindingSink) {
        sink.field("crate", &self.crate_name);
        sink.field("context", &self.context);
        sink.field("file", &self.span.file.display().to_string());
        sink.field("line", &self.span.line.to_string());
        sink.field("snippet", &self.snippet);
        sink.snippet(&self.snippet);
    }
}

#[derive(Debug, Clone)]
struct TodoRecord {
    context: String,
    file: std::path::PathBuf,
    line: u32,
    snippet: String,
}

#[derive(Debug, Default, Clone, Copy)]
struct TodoInventoryEnricher;

impl IrEnricher for TodoInventoryEnricher {
    fn id(&self) -> &str {
        "acme-todo-inventory"
    }

    fn enrich(
        &self,
        ir: &mut dyn IrMut,
        load: &dyn LoadView,
        session: &dyn SessionView,
    ) -> CordialResult<()> {
        let Some(source) = load.as_any().downcast_ref::<SourceLoadView>() else {
            return Ok(());
        };

        for file in &source.files {
            let syntax = syn::parse_file(&file.source).map_err(|err| {
                cordial::CordialError::syn_parse(file.path.display().to_string(), err)
            })?;
            let module_prefix = cordial::module_path_from_src_file(&source.src_root, &file.path);
            let mut visitor = TodoVisitor {
                module_prefix,
                fn_stack: Vec::new(),
                records: Vec::new(),
                file: file.path.clone(),
            };
            visitor.visit_file(&syntax);
            for record in visitor.records {
                let span = FileSpan::new(record.file.clone(), record.line, 1);
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
                        file.path
                            .strip_prefix(session.project_root())
                            .unwrap_or(&file.path)
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

struct TodoVisitor {
    module_prefix: Vec<String>,
    fn_stack: Vec<String>,
    records: Vec<TodoRecord>,
    file: std::path::PathBuf,
}

impl TodoVisitor {
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
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.fn_stack.push(node.sig.ident.to_string());
        syn::visit::visit_item_fn(self, node);
        self.fn_stack.pop();
    }

    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        if node.mac.path.is_ident("todo") {
            self.records.push(TodoRecord {
                context: self.context(),
                file: self.file.clone(),
                line: node.span().start().line as u32,
                snippet: node
                    .mac
                    .path
                    .get_ident()
                    .map(|id| format!("{id}!()"))
                    .unwrap_or_else(|| "todo!()".to_string()),
            });
        }
        syn::visit::visit_expr_macro(self, node);
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct TodoSitesQuery;

impl Query for TodoSitesQuery {
    fn node_kinds(&self) -> &[NodeKind] {
        &[NodeKind::Expr]
    }

    fn edge_kinds(&self) -> &[cordial::EdgeKind] {
        &[]
    }

    fn matches_node(&self, node: &dyn NodeView) -> bool {
        node.attr(TODO_ATTR).is_some()
    }
}

static TODO_SITES_QUERY: TodoSitesQuery = TodoSitesQuery;

#[derive(Debug, Default, Clone, Copy)]
struct TodoSiteProbe;

impl Probe for TodoSiteProbe {
    fn id(&self) -> &str {
        "acme-todo-site"
    }

    fn interests(&self) -> &dyn Query {
        &TODO_SITES_QUERY
    }

    fn probe(
        &self,
        ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Marker>>> {
        Ok(ir
            .nodes_matching(&TODO_SITES_QUERY)
            .into_iter()
            .map(|node| {
                Box::new(TodoMarker {
                    anchor: NodeAnchor(node.id),
                }) as Box<dyn Marker>
            })
            .collect())
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct TodoAssessor;

impl Assessor for TodoAssessor {
    fn id(&self) -> &str {
        "acme-todo-assessor"
    }

    fn consumes(&self) -> &[&str] {
        &[TODO_LABEL]
    }

    fn assess(
        &self,
        markers: &[&dyn Marker],
        ir: &dyn IrView,
        session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn Finding>>> {
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
                anchor: NodeAnchor(node_id),
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
    fn id(&self) -> &str {
        "acme-todo-csv"
    }

    fn render(
        &self,
        findings: &[&dyn Finding],
        _ir: &dyn IrView,
        _session: &dyn SessionView,
    ) -> CordialResult<Vec<Box<dyn cordial::Artifact>>> {
        let mut body = String::from("crate,context,file,line,snippet\n");
        for finding in findings {
            if finding.rule().id() != "ACME-TODO-001" {
                continue;
            }
            let mut sink = cordial::MapFindingSink::default();
            finding.emit(&mut sink);
            let field = |name: &str| {
                sink.fields
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
        Ok(vec![Box::new(TextArtifact {
            name: "acme-todo.csv".to_string(),
            media_type: "text/csv".to_string(),
            body,
        })])
    }
}
