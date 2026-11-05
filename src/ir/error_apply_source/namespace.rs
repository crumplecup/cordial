//! Module boundaries and import bindings collected with error declarations.

use crate::loader::{DatabaseSource, SharedParsedFile};
use ra_ap_syntax::ast::{self, HasAttrs, HasModuleItem, HasName};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ApplyNamespace {
    modules: Vec<ApplyModule>,
    imports: Vec<ApplyImport>,
    aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ApplyModule {
    name: String,
    #[getter(copy)]
    unconditional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(crate) struct ApplyImport {
    bindings: Vec<(String, Vec<String>)>,
    #[getter(copy)]
    unconditional: bool,
}

impl ApplyNamespace {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "ApplyNamespace")]
    pub(super) fn collect(file: &SharedParsedFile) -> Self {
        let mut facts = Self {
            modules: Vec::new(),
            imports: Vec::new(),
            aliases: Vec::new(),
        };
        for item in &file.syntax().items {
            match item {
                syn::Item::Mod(item)
                    if item.content.is_none()
                        && item.attrs.iter().all(|attr| {
                            attr.path().is_ident("doc") || attr.path().is_ident("cfg")
                        }) =>
                {
                    facts.modules.push(ApplyModule {
                        name: item.ident.to_string(),
                        unconditional: item.attrs.iter().all(|attr| attr.path().is_ident("doc")),
                    });
                }
                syn::Item::Use(item) => {
                    let mut bindings = Vec::new();
                    collect_syn_bindings(&item.tree, Vec::new(), &mut bindings);
                    facts.imports.push(ApplyImport {
                        bindings,
                        unconditional: item.attrs.iter().all(|attr| attr.path().is_ident("doc")),
                    });
                }
                syn::Item::Type(item) => facts.aliases.push(item.ident.to_string()),
                _ => {}
            }
        }
        facts
    }
}

#[tracing::instrument(level = "trace", skip(tree, paths))]
fn collect_syn_bindings(
    tree: &syn::UseTree,
    mut prefix: Vec<String>,
    paths: &mut Vec<(String, Vec<String>)>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_syn_bindings(&path.tree, prefix, paths);
        }
        syn::UseTree::Name(name) => {
            prefix.push(name.ident.to_string());
            paths.push((name.ident.to_string(), prefix));
        }
        syn::UseTree::Rename(rename) => {
            prefix.push(rename.ident.to_string());
            paths.push((rename.rename.to_string(), prefix));
        }
        syn::UseTree::Group(group) => {
            for tree in &group.items {
                collect_syn_bindings(tree, prefix.clone(), paths);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}

impl ApplyNamespace {
    #[tracing::instrument(level = "debug", skip(file))]
    #[hotpath::measure(impl_type = "ApplyNamespace")]
    pub(super) fn collect_native(file: &DatabaseSource<'_>) -> Self {
        let mut facts = Self {
            modules: Vec::new(),
            imports: Vec::new(),
            aliases: Vec::new(),
        };
        for item in file.tree().items() {
            match item {
                ast::Item::Module(item)
                    if item.item_list().is_none()
                        && item
                            .attrs()
                            .all(|attr| named(&attr, "doc") || named(&attr, "cfg")) =>
                {
                    if let Some(name) = item.name() {
                        facts.modules.push(ApplyModule {
                            name: name.text().to_string(),
                            unconditional: item.attrs().all(|attr| named(&attr, "doc")),
                        });
                    }
                }
                ast::Item::Use(item) => {
                    let mut bindings = Vec::new();
                    if let Some(tree) = item.use_tree() {
                        collect_native_bindings(&tree, Vec::new(), &mut bindings);
                    }
                    facts.imports.push(ApplyImport {
                        bindings,
                        unconditional: item.attrs().all(|attr| named(&attr, "doc")),
                    });
                }
                ast::Item::TypeAlias(item) => {
                    if let Some(name) = item.name() {
                        facts.aliases.push(name.text().to_string());
                    }
                }
                _ => {}
            }
        }
        facts
    }
}

#[tracing::instrument(level = "trace", skip(attr))]
fn named(attr: &ast::Attr, name: &str) -> bool {
    !matches!(attr.meta(), Some(ast::Meta::UnsafeMeta(_)))
        && attr.simple_name().as_deref() == Some(name)
        && attr
            .path()
            .is_none_or(|path| path.coloncolon_token().is_none())
}

#[tracing::instrument(level = "trace", skip(tree, paths))]
fn collect_native_bindings(
    tree: &ast::UseTree,
    mut prefix: Vec<String>,
    paths: &mut Vec<(String, Vec<String>)>,
) {
    if tree.star_token().is_some() {
        return;
    }
    if let Some(path) = tree.path() {
        let mut parts = Vec::new();
        let mut current = Some(path);
        while let Some(path) = current {
            if let Some(name) = path.segment().and_then(|part| part.name_ref()) {
                parts.push(name.text().to_string());
            }
            current = path.qualifier();
        }
        prefix.extend(parts.into_iter().rev());
    }
    if let Some(group) = tree.use_tree_list() {
        for child in group.use_trees() {
            collect_native_bindings(&child, prefix.clone(), paths);
        }
    } else if let Some(name) = tree
        .rename()
        .and_then(|rename| {
            rename
                .name()
                .map(|name| name.text().to_string())
                .or_else(|| rename.underscore_token().map(|_| "_".to_string()))
        })
        .or_else(|| prefix.last().cloned())
    {
        paths.push((name, prefix));
    }
}
