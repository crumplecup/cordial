//! Antipattern evidence from loader-issued canonical Rust syntax.
use super::catalog::{
    AntipatternCandidate, AntipatternCatalog, AntipatternSyntaxFacts, StaticField,
};
use super::{AntipatternRuleId, AntipatternSiteRecord};
use crate::loader::{DatabaseSource, module_path_from_src_file};
use crate::{CordialResult, FileSnapshot};
use ra_ap_syntax::{
    AstNode, SyntaxNode, ast,
    ast::{HasAttrs, HasName},
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
mod facts;
mod fields;
mod functions;
mod types;

#[derive(Default)]
struct CrateFacts {
    traits: HashSet<String>,
    errors: HashSet<String>,
    constants: HashSet<String>,
    runtime: HashSet<String>,
}

#[derive(Clone, Default)]
struct Context {
    modules: Vec<String>,
    implementation: Option<String>,
    functions: Vec<String>,
    foreign: bool,
    siblings: HashMap<String, HashSet<String>>,
}

struct Collector<'a, 'db> {
    source: &'a DatabaseSource<'db>,
    root: &'a Path,
    facts: &'a CrateFacts,
    sites: Vec<AntipatternCandidate>,
}

impl AntipatternCatalog {
    #[tracing::instrument(level = "debug", skip(files), err(level = "warn"))]
    #[hotpath::measure(impl_type = "AntipatternCatalog")]
    pub(crate) fn collect_native(
        root: &Path,
        crate_name: &str,
        files: &[(PathBuf, DatabaseSource<'_>)],
    ) -> CordialResult<Self> {
        let ordinary: Vec<_> = files
            .iter()
            .filter(|(_, source)| !crate::loader::path_has_fixtures(source.path(), root))
            .collect();
        let mut facts = CrateFacts::default();
        for (_, source) in &ordinary {
            facts.collect(source);
        }
        for (_, source) in &ordinary {
            facts.collect_kinds(source.tree().syntax());
        }
        let mut sites = Vec::new();
        let mut snapshots = Vec::new();
        for (tree, source) in ordinary {
            snapshots.push(FileSnapshot::new(
                source.path().to_path_buf(),
                source.source_digest(),
            ));
            let mut collector = Collector {
                source,
                root,
                facts: &facts,
                sites: Vec::new(),
            };
            let context = Context {
                modules: module_path_from_src_file(tree, source.path()),
                siblings: functions::siblings(source.tree().syntax()),
                ..Context::default()
            };
            collector.walk(source.tree().syntax(), &context)?;
            sites.extend(collector.sites);
        }
        let contracts =
            super::contract_bounds::collect_native(root, crate_name, files, &mut snapshots)?;
        snapshots.sort_by(|a, b| a.path().cmp(b.path()));
        snapshots.dedup();
        Ok(Self::new(
            snapshots,
            AntipatternSyntaxFacts::new(sites),
            contracts,
        ))
    }
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
            context.foreign = implementation
                .trait_()
                .as_ref()
                .map(types::label)
                .is_some_and(|name| !self.facts.traits.contains(&name));
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
        } else if let Some(alias) = ast::TypeAlias::cast(node.clone()) {
            if !alias
                .syntax()
                .parent()
                .is_some_and(|parent| ast::AssocItemList::can_cast(parent.kind()))
            {
                context
                    .functions
                    .extend(alias.name().map(|name| name.text().to_string()));
            }
        }
        if let Some(adt) = ast::Struct::cast(node.clone()) {
            if let Some(name) = adt.name() {
                self.fields(adt.field_list(), &name.text().to_string(), &context)?;
            }
        } else if let Some(adt) = ast::Enum::cast(node.clone()) {
            if let (Some(name), Some(list)) = (adt.name(), adt.variant_list()) {
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
