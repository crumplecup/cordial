//! Populate source nodes and semantic evidence in the same ordinary IR graph.

use std::collections::HashMap;

use ra_ap_hir::Semantics;
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{
    AstNode, TextRange,
    ast::{self, HasName},
};

use super::contexts::FunctionNodes;
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
    /// Every function item's node, for the cfg-context passes that follow.
    functions: FunctionNodes,
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
        (file, source): (ra_ap_vfs::FileId, &DatabaseSource<'_>),
        module_path: &str,
        graph: &mut CrateIr,
    ) -> CordialResult<NodeId> {
        let mut module = source_weight(source, NodeKind::Module, module_path, 1);
        let facts = queries::file_facts(
            db,
            queries::FileKey::new(db, file, source.path().to_path_buf()),
        )
        .as_ref()
        .ok_or_else(|| crate::CordialError::workspace_source_facts_missing("file facts"))?;
        module.set_attr(
            crate::ir::ATTR_SOURCE_MODULE_SCOPE,
            serde_json::to_value(facts.scope())?,
        );
        let attributes = facts.attributes();
        let declaration_paths: HashMap<_, _> = attributes
            .items()
            .iter()
            .map(|item| ((item.range().start(), item.range().end()), item.path()))
            .collect();
        module.set_attr(
            crate::ir::ATTR_SOURCE_FILE_ATTRIBUTES,
            serde_json::to_value(attributes)?,
        );
        module.set_attr("qualified_path", module_path.into());
        let module = graph.insert_node(module);
        graph.insert_edge(graph.root(), module, EdgeKind::Contains)?;
        #[cfg(feature = "tracing")]
        let macros = source.proof_macro_syntax();
        let mut nodes: HashMap<TextRange, (NodeId, String)> = HashMap::new();
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
                weight.set_attr(
                    crate::ir::ATTR_FUNCTION_LOCATION,
                    serde_json::to_value(crate::FunctionLocation::new(
                        source.path().to_path_buf(),
                        u32::from(range.start()),
                    ))?,
                );
            }
            let node = graph.insert_node(weight);
            graph.insert_edge(parent, node, EdgeKind::Contains)?;
            if matches!(item, ast::Item::Fn(_)) {
                self.functions
                    .insert(source.path(), item.syntax().text_range(), node);
            }
            if let Some(facts) = crate::IrView::semantic_function(graph, node)? {
                self.definitions.insert(*facts.definition(), node);
                for call in facts.calls() {
                    if let crate::SemanticCallTarget::Function(target) = call.target() {
                        self.calls.push((node, *target));
                    }
                }
            }
            #[cfg(feature = "tracing")]
            if let ast::Item::MacroCall(invocation) = &item {
                let reading = super::islands::Reading::new(db, sema, &macros, module_path);
                self.record_islands(&reading, source, invocation, node, graph)?;
            }
            nodes.insert(item.syntax().text_range(), (node, qualified));
        }
        Ok(module)
    }

    /// Give each function in a macro invocation's tokens a node under the
    /// invocation, with its gates and the calls resolved at the invocation.
    #[cfg(feature = "tracing")]
    #[tracing::instrument(
        level = "debug",
        skip(self, reading, source, invocation, graph, parent),
        err(level = "warn")
    )]
    #[hotpath::measure]
    fn record_islands(
        &mut self,
        reading: &super::islands::Reading<'_, '_>,
        source: &DatabaseSource<'_>,
        invocation: &ast::MacroCall,
        parent: NodeId,
        graph: &mut CrateIr,
    ) -> CordialResult<()> {
        let line = source.line(invocation.syntax().text_range().start());
        for function in super::islands::functions(reading.macros(), invocation) {
            let name = function
                .path()
                .rsplit("::")
                .next()
                .unwrap_or(function.path())
                .to_owned();
            let mut weight = source_weight(source, NodeKind::Item(ItemKind::Fn), &name, line);
            let qualified = if reading.module_path().is_empty() {
                function.path().clone()
            } else {
                format!("{}::{}", reading.module_path(), function.path())
            };
            weight.set_attr("qualified_path", qualified.into());
            weight.set_attr(
                crate::ir::ATTR_MACRO_ISLAND,
                serde_json::to_value(crate::ir::MacroIsland::new(function.gates().clone()))?,
            );
            let node = graph.insert_node(weight);
            graph.insert_edge(parent, node, EdgeKind::Contains)?;
            for path in function.calls() {
                if let Some(target) = super::islands::resolve(reading, invocation, path) {
                    self.calls.push((node, target));
                }
            }
        }
        Ok(())
    }

    /// Add the resolved `Calls` edges and hand back the function nodes.
    #[tracing::instrument(level = "trace", skip(self, graph, locator), err(level = "warn"))]
    pub(super) fn connect(
        self,
        graph: &mut CrateIr,
        locator: &super::external::Locator<'_>,
    ) -> CordialResult<FunctionNodes> {
        // One edge per caller and callee: the relationship. Each call's own
        // range and contract stay in the caller's semantic facts.
        let pairs: std::collections::BTreeSet<(NodeId, NodeId)> = self
            .calls
            .iter()
            .filter_map(|(caller, target)| {
                self.definitions
                    .get(target)
                    .map(|callee| (*caller, *callee))
            })
            .collect();
        for (caller, callee) in pairs {
            graph.insert_edge(caller, callee, EdgeKind::Calls)?;
        }
        for (id, node) in &self.definitions {
            locator.record_trait_implementation(graph, *id, *node)?;
        }
        // A call to a function with no node here may land in another crate of
        // the workspace; record where, so that crate's graph can claim it.
        let mut external: std::collections::BTreeMap<
            NodeId,
            std::collections::BTreeSet<crate::FunctionLocation>,
        > = std::collections::BTreeMap::new();
        for (caller, target) in &self.calls {
            if !self.definitions.contains_key(target)
                && let Some(location) = locator.locate(*target)
            {
                external.entry(*caller).or_default().insert(location);
            }
        }
        for (caller, locations) in external {
            let calls: Vec<_> = locations
                .into_iter()
                .map(|location| crate::ExternalCall::new(location, None))
                .collect();
            graph.set_attr(
                caller,
                crate::ATTR_EXTERNAL_CALLS,
                serde_json::to_value(calls)?,
            )?;
        }
        Ok(self.functions)
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
    let definition = sema
        .to_def(function)
        .or_else(|| expanded_definition(sema, function));
    if definition.is_none() {
        tracing::debug!(
            target: "cordial::semantic::definitions",
            function = %function.name().map(|name| name.text().to_string()).unwrap_or_default(),
            attribute_macro = sema.expand_attr_macro(&ast::Item::Fn(function.clone())).is_some(),
            "function has no definition in the database"
        );
    }
    if let Some(definition) = definition {
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

/// How many attribute macros may be stacked on one function before it is
/// reported as a gap. Every expansion that still carries a macro costs one.
const MAX_ATTRIBUTE_MACRO_DEPTH: usize = 8;

/// The definition of a function attribute macros replace, read from the function
/// they finally generate. With several macros stacked (`#[instrument]` over
/// `#[measure]`), each expansion still carries the next, so it is expanded again
/// until a definition appears. `None` when no macro applies or one cannot be
/// expanded (no proc-macro server, a macro error): that stays a gap, not a guess.
#[tracing::instrument(level = "trace", skip(sema, function))]
pub(super) fn expanded_definition(
    sema: &Semantics<'_, RootDatabase>,
    function: &ast::Fn,
) -> Option<ra_ap_hir::Function> {
    let name = function.name()?;
    let mut current = function.clone();
    for _ in 0..MAX_ATTRIBUTE_MACRO_DEPTH {
        let expansion = sema.expand_attr_macro(&ast::Item::Fn(current.clone()))?;
        let generated = expansion
            .value
            .value
            .descendants()
            .filter_map(ast::Fn::cast)
            .find(|candidate| {
                candidate
                    .name()
                    .is_some_and(|candidate| candidate.text() == name.text())
            })?;
        if let Some(definition) = sema.to_def(&generated) {
            return Some(definition);
        }
        current = generated;
    }
    None
}
