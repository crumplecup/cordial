//! Antipattern evidence from loader-issued canonical Rust syntax.
use super::catalog::{
    AntipatternCandidate, AntipatternCatalog, AntipatternSyntaxFacts, StaticField, StaticFieldShape,
};
use super::{AntipatternRuleId, AntipatternSiteRecord};
use crate::loader::{DatabaseSource, module_path_from_src_file};
use crate::{CordialResult, FileSnapshot};
use ra_ap_syntax::{
    AstNode, SyntaxNode, ast,
    ast::{HasAttrs, HasName},
};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
};
mod facts;
mod fields;
mod functions;
mod types;

pub(crate) use facts::FileFacts;

/// Shared selection for production collection and Cargo-backed projection.
#[tracing::instrument(level = "debug", skip(root, held))]
#[hotpath::measure]
pub(crate) fn input_paths(
    root: &Path,
    contracts: bool,
    held: &crate::loader::held_files::HeldFiles,
) -> Vec<(PathBuf, PathBuf)> {
    let mut paths = Vec::new();
    for tree in held.scan_trees(root) {
        for path in held.under(&tree) {
            if crate::loader::path_has_fixtures(path, root)
                && !(contracts
                    && path.starts_with(root.join("src"))
                    && !path.components().any(|part| part.as_os_str() == "gallery"))
            {
                continue;
            }
            paths.push((tree.clone(), path.clone()));
        }
    }
    paths.sort_by(|a, b| a.1.cmp(&b.1));
    paths
}

#[derive(serde::Serialize, serde::Deserialize)]
/// The crate-wide relationships the per-file walk reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub(crate) struct CrateFacts {
    traits: BTreeSet<String>,
    errors: BTreeSet<String>,
    constants: BTreeSet<String>,
    runtime: BTreeSet<String>,
}

#[derive(Clone, Default)]
struct Context {
    modules: Vec<String>,
    implementation: Option<String>,
    functions: Vec<String>,
    /// The trait of the `impl` the walk is in, if it is one of a trait.
    owner_trait: Option<String>,
    siblings: HashMap<String, HashSet<String>>,
}

struct Collector<'a, 'db> {
    source: &'a DatabaseSource<'db>,
    root: &'a Path,
    sites: Vec<PendingCandidate>,
}

/// An antipattern candidate before the crate's facts are applied.
///
/// A file's walk does not read the crate's facts: it keeps what the facts would decide, and the
/// decision is made afterwards over the candidates alone (see [`PendingCandidate::resolve`]), so
/// the file's syntax is read once for the facts and the candidates.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum PendingCandidate {
    /// Not decided by the facts.
    Settled(AntipatternCandidate),
    /// A site in a function of an `impl` of `trait_name`: the policy owns the function only if the
    /// crate declares that trait, and a foreign trait's signature is not the crate's to choose.
    OfTrait {
        trait_name: String,
        site: AntipatternSiteRecord,
    },
    /// A `&'static` field of `type_name`, which the facts decide: whether it is disallowed (under the
    /// crate's traits), whether the type is a constant and not a runtime value (then it is not
    /// reported), and whether it is an error type.
    Field {
        site: AntipatternSiteRecord,
        shape: StaticFieldShape,
        type_name: String,
        location: bool,
        /// For each `&'static` in the field's type, the traits its spine names, or none where the
        /// reference has no type, which no trait excuses.
        references: Vec<Option<Vec<String>>>,
    },
}

impl PendingCandidate {
    /// The candidates the facts leave, as the walk would have found them reading the facts.
    #[tracing::instrument(level = "debug", skip_all)]
    #[hotpath::measure]
    pub(crate) fn resolve(pending: &[Self], facts: &CrateFacts) -> Vec<AntipatternCandidate> {
        pending
            .iter()
            .filter_map(|candidate| match candidate {
                Self::Settled(found) => Some(found.clone()),
                Self::OfTrait { trait_name, site } => facts
                    .traits
                    .contains(trait_name)
                    .then(|| AntipatternCandidate::Record(site.clone())),
                Self::Field {
                    site,
                    shape,
                    type_name,
                    location,
                    references,
                } => {
                    let disallowed = references.iter().any(|spine| {
                        spine.as_ref().is_none_or(|names| {
                            !names.iter().any(|name| facts.traits.contains(name))
                        })
                    });
                    if !disallowed
                        || (!location
                            && facts.constants.contains(type_name)
                            && !facts.runtime.contains(type_name))
                    {
                        return None;
                    }
                    Some(AntipatternCandidate::StaticField(StaticField::new(
                        site.clone(),
                        facts.errors.contains(type_name),
                        *shape,
                    )))
                }
            })
            .collect()
    }
}

impl AntipatternCatalog {
    #[tracing::instrument(level = "debug", skip(files), err(level = "warn"))]
    #[hotpath::measure(impl_type = "AntipatternCatalog")]
    pub(crate) fn collect(
        root: &Path,
        crate_name: &str,
        files: &[(PathBuf, &DatabaseSource<'_>)],
    ) -> CordialResult<Self> {
        let ordinary: Vec<_> = files
            .iter()
            .filter(|(_, source)| !crate::loader::path_has_fixtures(source.path(), root))
            .collect();
        let per_file: Vec<FileFacts> = ordinary
            .iter()
            .map(|(_, source)| FileFacts::collect(source))
            .collect();
        let facts = CrateFacts::join(per_file.iter());
        let mut sites = Vec::new();
        let mut snapshots = Vec::new();
        for (tree, source) in ordinary {
            snapshots.push(FileSnapshot::new(
                source.path().to_path_buf(),
                source.source_digest(),
            ));
            sites.extend(file_candidates(source, tree, root, &facts)?);
        }
        let contracts = super::contract_bounds::collect(root, crate_name, files, &mut snapshots)?;
        Ok(Self::from_parts(snapshots, sites, contracts))
    }
}

/// Single-file adapters retain fixture behavior without excluding fixture paths.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
#[hotpath::measure]
pub(super) fn collect_file(
    source: &DatabaseSource<'_>,
    tree: &Path,
    root: &Path,
) -> CordialResult<AntipatternSyntaxFacts> {
    let facts = CrateFacts::join(std::iter::once(&FileFacts::collect(source)));
    Ok(AntipatternSyntaxFacts::new(file_candidates(
        source, tree, root, &facts,
    )?))
}

/// One file's antipattern candidates, given the crate's facts: the candidates of the file's syntax
/// and those facts.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
#[hotpath::measure]
pub(crate) fn file_candidates(
    source: &DatabaseSource<'_>,
    tree: &Path,
    root: &Path,
    facts: &CrateFacts,
) -> CordialResult<Vec<AntipatternCandidate>> {
    Ok(PendingCandidate::resolve(
        &pending_candidates(source, tree, root)?,
        facts,
    ))
}

/// One file's antipattern candidates before the crate's facts are applied: a function of the file's
/// syntax alone.
#[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
#[hotpath::measure]
pub(crate) fn pending_candidates(
    source: &DatabaseSource<'_>,
    tree: &Path,
    root: &Path,
) -> CordialResult<Vec<PendingCandidate>> {
    let mut collector = Collector {
        source,
        root,
        sites: Vec::new(),
    };
    let context = Context {
        modules: module_path_from_src_file(tree, source.path()),
        siblings: functions::siblings(source.tree().syntax()),
        ..Context::default()
    };
    collector.walk(source.tree().syntax(), &context)?;
    Ok(collector.sites)
}

impl Context {
    #[tracing::instrument(level = "trace", skip(self))]
    fn label(&self) -> String {
        let mut parts = self.modules.clone();
        parts.extend(self.implementation.clone());
        parts.extend(self.functions.clone());
        if parts.is_empty() {
            "<crate>".into()
        } else {
            parts.join("::")
        }
    }
}

impl Collector<'_, '_> {
    #[tracing::instrument(level = "debug", skip_all, err(level = "warn"))]
    #[hotpath::measure]
    fn walk(&mut self, node: &SyntaxNode, inherited: &Context) -> CordialResult<()> {
        // Closures and trait methods have never participated in this policy.
        if ast::ClosureExpr::can_cast(node.kind()) {
            return Ok(());
        }
        let mut context = inherited.clone();
        if let Some(module) = ast::Module::cast(node.clone()) {
            if cfg_test(&module) {
                return Ok(());
            }
            context
                .modules
                .extend(module.name().map(|name| name.text().to_string()));
            if let Some(list) = module.item_list() {
                context.siblings = functions::siblings(list.syntax());
            }
        } else if let Some(implementation) = ast::Impl::cast(node.clone()) {
            context.implementation = implementation.self_ty().as_ref().map(types::label);
            context.owner_trait = implementation.trait_().as_ref().map(types::label);
            if let Some(list) = implementation.assoc_item_list() {
                context.siblings = functions::siblings(list.syntax());
            }
        } else if let Some(function) = ast::Fn::cast(node.clone()) {
            if function
                .syntax()
                .parent()
                .and_then(|parent| parent.parent())
                .is_some_and(|owner| ast::Trait::can_cast(owner.kind()))
            {
                return Ok(());
            }
            // Foreign declarations have types, but no policy-owned body or
            // parameter contract; preserve their enclosing type context.
            if function.body().is_some() {
                context
                    .functions
                    .extend(function.name().map(|name| name.text().to_string()));
                self.function(&function, &context)?;
            }
        } else if let Some(alias) = ast::TypeAlias::cast(node.clone())
            && !alias
                .syntax()
                .parent()
                .is_some_and(|parent| ast::AssocItemList::can_cast(parent.kind()))
        {
            context
                .functions
                .extend(alias.name().map(|name| name.text().to_string()));
        }
        if let Some(adt) = ast::Struct::cast(node.clone()) {
            if let Some(name) = adt.name() {
                self.fields(adt.field_list(), name.text(), &context)?;
            }
        } else if let Some(adt) = ast::Enum::cast(node.clone())
            && let (Some(name), Some(list)) = (adt.name(), adt.variant_list())
        {
            for variant in list.variants() {
                if let Some(variant_name) = variant.name() {
                    self.fields(
                        variant.field_list(),
                        &format!("{}::{}", name.text(), variant_name.text()),
                        &context,
                    )?;
                }
            }
        }
        if let Some(ty) = ast::Type::cast(node.clone()) {
            self.error_type(&ty, &context)?;
        }
        for child in node.children() {
            self.walk(&child, &context)?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self, rule), err(level = "warn"))]
    fn site(
        &self,
        rule: AntipatternRuleId,
        context: String,
        line: u32,
        snippet: String,
    ) -> CordialResult<AntipatternSiteRecord> {
        AntipatternSiteRecord::builder()
            .rule_id(rule)
            .context(context)
            .file(
                self.source
                    .path()
                    .strip_prefix(self.root)
                    .unwrap_or(self.source.path())
                    .to_path_buf(),
            )
            .line(line)
            .snippet(snippet)
            .build()
    }
}

#[tracing::instrument(level = "trace", skip(owner))]
fn cfg_test(owner: &dyn HasAttrs) -> bool {
    owner
        .attrs()
        .any(|attr| compact(attr.syntax()) == "#[cfg(test)]")
}

#[tracing::instrument(level = "trace", skip(node))]
fn compact(node: &SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|part| part.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| token.text().to_string())
        .collect()
}
