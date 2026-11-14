//! One file's items as the IR will hold them, as a Salsa query.
//!
//! Projecting a file into the graph read its syntax, built a weight for every
//! item (names, lines, qualified paths, the function's syntax facts, its semantic
//! evidence as JSON) and inserted the nodes. Everything but the insertion is a
//! function of the file's text and of what its functions resolve to, so it is
//! computed here once and kept: a session builds it again only for a file that
//! changed, or whose functions' evidence did. The projection then only inserts the
//! kept nodes and edges, in the order they were built, so node ids are unchanged.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ra_ap_hir::Semantics;
use ra_ap_ide_db::RootDatabase;
use ra_ap_syntax::{
    AstNode, TextRange,
    ast::{self, HasName},
};
use tracing::instrument;

use super::populate::expanded_definition;
use super::queries::{self, FileKey, SemanticDatabase};
use crate::ir::{Resolved, TargetFacts, resolve_call};
use crate::loader::{SourceLoader, module_path_from_src_file};
use crate::{
    CordialError, CordialResult, ItemKind, NodeKind, NodeWeight, SemanticCallTarget, SemanticItemId,
};

use super::DatabaseSource;

/// A source file as the item query reads it: the database's id for it, the path
/// the IR names it by, and the crate's `src` directory, which names the module of
/// a file outside the module tree.
#[salsa::interned]
pub(super) struct ItemsKey<'db> {
    #[returns(copy)]
    file: ra_ap_vfs::FileId,
    #[returns(ref)]
    path: PathBuf,
    #[returns(ref)]
    src: PathBuf,
}

/// A function found in the tokens of an item-position macro invocation.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub(super) struct IslandRecord {
    /// The node it will have, a child of the invocation's.
    weight: NodeWeight,
    /// The functions it calls, as the database resolved them.
    calls: Vec<SemanticItemId>,
}

/// What the observability pass reads of a function's ordinary facts: the callees
/// of its calls and whether it returns a `Result`. Typed here, so the pass does not
/// decode the whole facts back out of the graph for every function.
#[cfg(feature = "tracing")]
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub(in crate::loader::database) struct ObservedCalls {
    callees: Vec<crate::ir::SemanticCallee>,
    fallible: bool,
}

/// The calls of a top-level free function, resolved against the file's own items.
/// A target is the place of its item in the file's list, because node ids do not
/// exist until the projection inserts the items.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub(super) struct RecordedFlow {
    source_digest: [u8; 32],
    /// Whether the file's own module scope is one names can be trusted in. The
    /// crate-wide part of that decision is made once every file is in.
    #[getter(copy)]
    trusted: bool,
    calls: Vec<(crate::SourceByteRange, Resolved<usize>)>,
}

/// One item of a file: its node weight and where it hangs.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub(super) struct ItemRecord {
    /// The enclosing item's place in the file's list, or `None` for the module.
    #[getter(copy)]
    parent: Option<usize>,
    weight: NodeWeight,
    /// For a function: the range of its item, which the cfg-context passes find
    /// its node by.
    function: Option<TextRange>,
    /// The function's identity and the functions it calls, when it resolved.
    definition: Option<SemanticItemId>,
    calls: Vec<SemanticItemId>,
    /// Functions read from an invocation's tokens.
    islands: Vec<IslandRecord>,
    /// How the calls of a top-level free function resolved.
    flow: Option<RecordedFlow>,
    /// What the observability pass reads of the function's ordinary facts.
    #[cfg(feature = "tracing")]
    observed: Option<ObservedCalls>,
}

/// Everything a file adds to the graph: its module node, and its items in order.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters)]
pub(super) struct FileItems {
    module: NodeWeight,
    items: Vec<ItemRecord>,
    /// Whether this file makes names in the crate untrustworthy: it declares
    /// `std` or `core`, or its scope has extern crate aliases or is opaque at the
    /// root. One such file withdraws trust from every call in the crate.
    #[getter(copy)]
    shadows_extern_crates: bool,
}

/// The file's items, or `None` when it did not parse, which the projection has
/// already refused (and, called without the query, reports).
#[salsa::tracked(returns(ref))]
#[instrument(level = "debug", skip(db, key))]
#[hotpath::measure]
pub(super) fn file_items<'db>(
    db: &'db dyn SemanticDatabase,
    key: ItemsKey<'db>,
) -> Option<FileItems> {
    tracing::debug!(target: "cordial::semantic::queries", function = %key.path(db).display(), query = "file_items", "semantic query executed");
    collect_items(db.root_database(), key.file(db), key.path(db), key.src(db)).ok()
}

/// Read a file's items, outside the query. The projection calls it when the
/// query declines a file, to report why.
#[instrument(level = "debug", skip(db), err(level = "warn"))]
#[hotpath::measure]
pub(super) fn collect_items(
    db: &RootDatabase,
    file: ra_ap_vfs::FileId,
    path: &Path,
    src: &Path,
) -> CordialResult<FileItems> {
    let sema = Semantics::new(db);
    let source = super::projection::source_of(db, &sema, file, path)?;
    let module_path = module_path_of(db, &sema, file, path, src);
    let facts = queries::file_facts(db, FileKey::new(db, file, path.to_path_buf()))
        .as_ref()
        .ok_or_else(|| CordialError::workspace_source_facts_missing("file facts"))?;
    let mut module = source_weight(&source, NodeKind::Module, &module_path, 1);
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
    module.set_attr("qualified_path", module_path.as_str().into());
    #[cfg(feature = "tracing")]
    let macros = source.proof_macro_syntax();
    let mut positions: HashMap<TextRange, (usize, String)> = HashMap::new();
    let mut items: Vec<ItemRecord> = Vec::new();
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
        let enclosing = item
            .syntax()
            .ancestors()
            .skip(1)
            .find_map(|ancestor| positions.get(&ancestor.text_range()).cloned());
        let (parent, path_of_parent) = match enclosing {
            Some((index, path)) => (Some(index), path),
            None => (None, module_path.clone()),
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
        } else if path_of_parent.is_empty() {
            name.clone()
        } else {
            format!("{path_of_parent}::{name}")
        };
        let mut weight = source_weight(
            &source,
            kind,
            &name,
            source.line(item.syntax().text_range().start()),
        );
        weight.set_attr("qualified_path", qualified.clone().into());
        #[cfg(feature = "tracing")]
        let mut observed = None;
        let (mut function, mut definition, mut calls) = (None, None, Vec::new());
        if let ast::Item::Fn(declared) = &item {
            if let Some(collected) = collect_function(db, &sema, &source, declared, &mut weight)? {
                definition = Some(collected.identity);
                calls = collected.targets;
                #[cfg(feature = "tracing")]
                {
                    observed = Some(collected.observed);
                }
            }
            weight.set_attr(
                crate::ir::ATTR_FUNCTION_LOCATION,
                serde_json::to_value(crate::FunctionLocation::new(
                    source.path().to_path_buf(),
                    u32::from(range.start()),
                ))?,
            );
            function = Some(range);
        }
        #[cfg(feature = "tracing")]
        let islands = match &item {
            ast::Item::MacroCall(invocation) => {
                let reading = super::islands::Reading::new(db, &sema, &macros, &module_path);
                read_islands(&reading, &source, invocation)?
            }
            _ => Vec::new(),
        };
        #[cfg(not(feature = "tracing"))]
        let islands = Vec::new();
        positions.insert(range, (items.len(), qualified));
        items.push(ItemRecord {
            parent,
            weight,
            function,
            definition,
            calls,
            islands,
            flow: None,
            #[cfg(feature = "tracing")]
            observed,
        });
    }
    let shadows_extern_crates = resolve_flows(facts.scope(), &module_path, &mut items)?;
    Ok(FileItems {
        module,
        items,
        shadows_extern_crates,
    })
}

/// Resolve the calls of every top-level free function that has semantic facts,
/// against the file's other items. Returns whether the file withdraws trust from
/// the crate's names.
#[instrument(level = "debug", skip(scope, items), err(level = "warn"))]
#[hotpath::measure]
fn resolve_flows(
    scope: &crate::ir::SourceModuleScope,
    module_path: &str,
    items: &mut [ItemRecord],
) -> CordialResult<bool> {
    // In older editions a root declaration can shadow an absolute standard-library
    // path, and root macros or attributes can introduce extern aliases that
    // affect absolute paths in files below the root. Refuse these in every edition.
    let shadows = matches!(module_path, "std" | "core")
        || items
            .iter()
            .any(|item| matches!(item.weight.name().as_deref(), Some("std" | "core")))
        || scope.extern_crates()
        || (module_path.is_empty() && scope.opaque());
    let trusted = !scope.opaque();
    let syntax: Vec<Option<crate::ir::FunctionSourceSyntax>> = items
        .iter()
        .map(|item| {
            item.weight
                .attr(crate::ir::ATTR_FUNCTION_SOURCE_SYNTAX)
                .map(|value| serde_json::from_value(value.clone()))
                .transpose()
        })
        .collect::<Result<_, _>>()?;
    let by_definition: HashMap<SemanticItemId, usize> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| Some((item.definition?, index)))
        .collect();
    let mut flows = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let (Some(facts), true) = (&syntax[index], item.parent.is_none()) else {
            continue;
        };
        let Some(semantic) = item
            .weight
            .attr(crate::ATTR_SEMANTIC_FUNCTION)
            .map(|value| serde_json::from_value::<crate::SemanticFunction>(value.clone()))
            .transpose()?
        else {
            continue;
        };
        let mut calls = Vec::new();
        for call in facts.calls() {
            let resolved = resolve_call(trusted, facts, call, Some(&semantic), &mut |target| {
                Ok(by_definition.get(&target).map(|place| TargetFacts {
                    key: *place,
                    beside_the_caller: items[*place].parent.is_none(),
                    is_function: items[*place].weight.kind() == &NodeKind::Item(ItemKind::Fn),
                    syntax: syntax[*place].clone(),
                }))
            })?;
            calls.push((call.range().clone(), resolved));
        }
        flows.push((
            index,
            RecordedFlow {
                source_digest: facts.source_digest(),
                trusted,
                calls,
            },
        ));
    }
    for (index, flow) in flows {
        items[index].flow = Some(flow);
    }
    Ok(shadows)
}

/// The module path of a file: its module's path from the crate root, or, for a
/// file outside the module tree (a stray or unreferenced source), the path its
/// location implies, as the policy scans always gave it.
#[instrument(level = "debug", skip(db, sema))]
#[hotpath::measure]
fn module_path_of(
    db: &RootDatabase,
    sema: &Semantics<'_, RootDatabase>,
    file: ra_ap_vfs::FileId,
    path: &Path,
    src: &Path,
) -> String {
    sema.file_to_module_defs(file)
        .next()
        .map(|module| {
            module
                .path_to_root(db)
                .into_iter()
                .rev()
                .filter_map(|module| module.name(db))
                .map(|name| name.as_str().to_owned())
                .collect::<Vec<_>>()
                .join("::")
        })
        .unwrap_or_else(|| module_path_from_src_file(src, path).join("::"))
}

/// Give each function in a macro invocation's tokens a node under the
/// invocation, with its gates and the calls resolved at the invocation.
#[cfg(feature = "tracing")]
#[instrument(
    level = "debug",
    skip(reading, source, invocation),
    err(level = "warn")
)]
#[hotpath::measure]
fn read_islands(
    reading: &super::islands::Reading<'_, '_>,
    source: &DatabaseSource<'_>,
    invocation: &ast::MacroCall,
) -> CordialResult<Vec<IslandRecord>> {
    let line = source.line(invocation.syntax().text_range().start());
    let mut found = Vec::new();
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
        let calls = function
            .calls()
            .iter()
            .filter_map(|path| super::islands::resolve(reading, invocation, path))
            .collect();
        found.push(IslandRecord { weight, calls });
    }
    Ok(found)
}

#[instrument(level = "trace", skip(source, kind))]
fn source_weight(source: &DatabaseSource<'_>, kind: NodeKind, name: &str, line: u32) -> NodeWeight {
    let mut weight = NodeWeight::new(kind)
        .with_name(name)
        .with_span(crate::FileSpan::new(source.path(), line, 1));
    weight.set_attr(SourceLoader::ATTR_IR_ORIGIN, SourceLoader::ORIGIN.into());
    weight
}

#[instrument(level = "trace", skip(item))]
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

/// What a function's collection found: the identity it resolved to, the functions
/// its evidence says it calls, and what the observability pass reads.
struct Collected {
    identity: SemanticItemId,
    targets: Vec<SemanticItemId>,
    #[cfg(feature = "tracing")]
    observed: ObservedCalls,
}

/// Add a function's syntax facts and semantic evidence to its weight, and return
/// the identity it resolved to and the functions its evidence says it calls.
#[instrument(
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
) -> CordialResult<Option<Collected>> {
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
    let Some(definition) = definition else {
        return Ok(None);
    };
    let facts = queries::evidence_of(db, definition)
        .as_ref()
        .ok_or_else(|| CordialError::workspace_source_facts_missing("semantic function source"))?;
    weight.set_attr(crate::ATTR_SEMANTIC_FUNCTION, serde_json::to_value(facts)?);
    let targets = facts
        .calls()
        .iter()
        .filter_map(|call| match call.target() {
            SemanticCallTarget::Function(target) => Some(*target),
            _ => None,
        })
        .collect();
    Ok(Some(Collected {
        identity: *facts.definition(),
        targets,
        #[cfg(feature = "tracing")]
        observed: ObservedCalls {
            callees: facts
                .calls()
                .iter()
                .filter_map(|call| call.callee().clone())
                .collect(),
            fallible: matches!(
                facts.result_recording(),
                crate::SemanticResultRecording::Result { .. }
            ),
        },
    }))
}
