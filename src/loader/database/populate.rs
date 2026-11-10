//! Populate source nodes and semantic evidence in the same ordinary IR graph.

use std::collections::HashMap;

use ra_ap_hir::Semantics;
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{
    AstNode, TextRange,
    ast::{self, HasName},
};

use super::{DatabaseSource, queries};
use crate::loader::SourceLoader;
use crate::{
    CordialResult, CrateIr, EdgeKind, ItemKind, NodeId, NodeKind, NodeWeight, SemanticItemId,
};

/// Graph-local lookup tables, discarded once this projection is complete.
#[derive(Default)]
pub(super) struct Projection {
    definitions: HashMap<SemanticItemId, NodeId>,
    calls: Vec<(NodeId, SemanticItemId)>,
}

impl Projection {
    #[tracing::instrument(
        level = "debug",
        skip(self, db, sema, source, graph),
        err(level = "warn")
    )]
    #[hotpath::measure(impl_type = "Projection")]
    pub(super) fn populate(
        &mut self,
        db: &RootDatabase,
        sema: &Semantics<'_, RootDatabase>,
        source: &DatabaseSource<'_>,
        module_path: &str,
        graph: &mut CrateIr,
    ) -> CordialResult<()> {
        let mut module = source_weight(source, NodeKind::Module, module_path, 1);
        module.set_attr(
            crate::ir::ATTR_SOURCE_MODULE_SCOPE,
            serde_json::to_value(crate::ir::SourceModuleScope::collect(source))?,
        );
        let attributes = crate::ir::SourceFileAttributes::collect(source);
        let declaration_paths: HashMap<_, _> = attributes
            .items()
            .iter()
            .map(|item| ((item.range().start(), item.range().end()), item.path()))
            .collect();
        module.set_attr(
            crate::ir::ATTR_SOURCE_FILE_ATTRIBUTES,
            serde_json::to_value(&attributes)?,
        );
        module.set_attr("qualified_path", module_path.into());
        let module = graph.insert_node(module);
        graph.insert_edge(graph.root(), module, EdgeKind::Contains)?;
        let mut nodes: HashMap<TextRange, (NodeId, String)> = HashMap::new();
        #[cfg(feature = "tracing")]
        let mut tracing_semantics = HashMap::new();
        for item in source
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Item::cast)
            .filter(|item| {
                !matches!(item, ast::Item::MacroCall(_))
                    || !item
                        .syntax()
                        .parent()
                        .is_some_and(|parent| ast::MacroExpr::can_cast(parent.kind()))
            })
        {
            let (name, kind) = item_kind(&item);
            let parent = item
                .syntax()
                .ancestors()
                .skip(1)
                .find_map(|ancestor| nodes.get(&ancestor.text_range()));
            let (parent, path) = match parent {
                Some((node, path)) => (*node, path.as_str()),
                None => (module, module_path),
            };
            let range = item.syntax().text_range();
            // Attribute and tracing consumers share the declaration's canonical
            // file-local path. Join by range, never a method's spelling or line.
            let local = declaration_paths.get(&(
                u32::from(range.start()) as usize,
                u32::from(range.end()) as usize,
            ));
            let qualified = if let Some(local) = local {
                if module_path.is_empty() {
                    (*local).clone()
                } else {
                    format!("{module_path}::{local}")
                }
            } else if path.is_empty() {
                name.clone()
            } else {
                format!("{path}::{name}")
            };
            let mut weight = source_weight(
                source,
                kind,
                &name,
                source.line(item.syntax().text_range().start()),
            );
            weight.set_attr("qualified_path", qualified.clone().into());
            if let ast::Item::Fn(function) = &item {
                collect_function(db, sema, source, function, &mut weight)?;
            }
            let node = graph.insert_node(weight);
            graph.insert_edge(parent, node, EdgeKind::Contains)?;
            if let Some(facts) = crate::IrView::semantic_function(graph, node)? {
                #[cfg(feature = "tracing")]
                tracing_semantics.insert(item.syntax().text_range(), facts.clone());
                self.definitions.insert(*facts.definition(), node);
                for call in facts.calls() {
                    if let crate::SemanticCallTarget::Function(target) = call.target() {
                        self.calls.push((node, *target));
                    }
                }
            }
            nodes.insert(item.syntax().text_range(), (node, qualified));
        }
        #[cfg(feature = "tracing")]
        graph.set_attr(
            module,
            crate::ir::ATTR_SOURCE_TRACING_INVENTORY,
            serde_json::to_value(crate::ir::SourceTracingInventory::collect(
                source,
                &attributes,
                &tracing_semantics,
            )?)?,
        )?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self, graph), err(level = "warn"))]
    pub(super) fn connect(self, graph: &mut CrateIr) -> CordialResult<()> {
        for (caller, target) in self.calls {
            if let Some(callee) = self.definitions.get(&target) {
                graph.insert_edge(caller, *callee, EdgeKind::Calls)?;
            }
        }
        Ok(())
    }
}

#[tracing::instrument(level = "trace", skip(source, kind))]
fn source_weight(source: &DatabaseSource<'_>, kind: NodeKind, name: &str, line: u32) -> NodeWeight {
    let mut weight = NodeWeight::new(kind)
        .with_name(name)
        .with_span(crate::FileSpan::new(source.path(), line, 1));
    weight.set_attr(SourceLoader::ATTR_IR_ORIGIN, SourceLoader::ORIGIN.into());
    weight
}

#[tracing::instrument(level = "trace", skip(item))]
fn item_kind(item: &ast::Item) -> (String, NodeKind) {
    let (name, kind) = match item {
        ast::Item::Fn(item) => (item.name(), ItemKind::Fn),
        ast::Item::Struct(item) => (item.name(), ItemKind::Struct),
        ast::Item::Enum(item) => (item.name(), ItemKind::Enum),
        ast::Item::Trait(item) => (item.name(), ItemKind::Trait),
        ast::Item::TypeAlias(item) => (item.name(), ItemKind::TypeAlias),
        ast::Item::Const(item) => (item.name(), ItemKind::Const),
        ast::Item::Static(item) => (item.name(), ItemKind::Static),
        ast::Item::Module(item) => (item.name(), ItemKind::Mod),
        ast::Item::MacroRules(item) => (item.name(), ItemKind::Macro),
        ast::Item::MacroDef(item) => (item.name(), ItemKind::Macro),
        ast::Item::MacroCall(_) => return ("macro".into(), NodeKind::Item(ItemKind::Macro)),
        ast::Item::Impl(item) => {
            return (
                format!("impl@{}", u32::from(item.syntax().text_range().start())),
                NodeKind::ImplBlock,
            );
        }
        _ => (None, ItemKind::Other),
    };
    (
        name.map(|name| name.text().to_string())
            .unwrap_or_else(|| "item".into()),
        NodeKind::Item(kind),
    )
}

#[tracing::instrument(
    level = "trace",
    skip(db, sema, source, function, weight),
    err(level = "warn")
)]
fn collect_function(
    db: &RootDatabase,
    sema: &Semantics<'_, RootDatabase>,
    source: &DatabaseSource<'_>,
    function: &ast::Fn,
    weight: &mut NodeWeight,
) -> CordialResult<()> {
    // The current transform contract describes top-level free
    // functions. Methods/nested items retain their own nodes and
    // semantic facts without acquiring a free-function contract.
    if function
        .syntax()
        .parent()
        .is_some_and(|parent| ast::SourceFile::can_cast(parent.kind()))
        && let Some(facts) =
            crate::ir::FunctionSourceSyntax::collect(source, function.syntax().text_range())
    {
        weight.set_attr(
            crate::ir::ATTR_FUNCTION_SOURCE_SYNTAX,
            serde_json::to_value(facts)?,
        );
    }
    if let Some(definition) = sema.to_def(function) {
        let key = queries::FunctionKey::new(db, definition);
        let facts = queries::function_evidence(db, key)
            .as_ref()
            .ok_or_else(|| {
                crate::CordialError::workspace_source_facts_missing("semantic function source")
            })?;
        weight.set_attr(crate::ATTR_SEMANTIC_FUNCTION, serde_json::to_value(facts)?);
    }
    Ok(())
}
