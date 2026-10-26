//! Located identifiers and paths gathered once from a shared source tree.
//! Graph closure uses these facts through the crate index instead of visiting
//! each file again for every candidate graph.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use syn::visit::Visit;
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::super::names::{NameAudit, audit_names};

pub(super) type LocatedName = (String, usize, usize);

#[derive(Debug, Clone, Serialize, Deserialize, derive_getters::Getters)]
pub(super) struct FileSyntaxFacts {
    path: std::path::PathBuf,
    #[getter(copy)]
    source_digest: [u8; 32],
    identifiers: BTreeSet<LocatedName>,
    declarations: BTreeSet<LocatedName>,
    paths: Vec<(LocatedName, Vec<String>)>,
    macro_names: BTreeSet<String>,
    #[getter(copy)]
    has_cfg: bool,
    name_audit: NameAudit,
}

impl FileSyntaxFacts {
    #[instrument(level = "debug", skip(file, tracked_names))]
    pub(super) fn collect(file: &SharedParsedFile, tracked_names: &BTreeSet<String>) -> Self {
        let mut collector = Collector {
            facts: Self {
                path: file.path().to_path_buf(),
                source_digest: file.source_digest(),
                identifiers: BTreeSet::new(),
                declarations: BTreeSet::new(),
                paths: Vec::new(),
                macro_names: BTreeSet::new(),
                has_cfg: false,
                name_audit: audit_names(file.syntax()),
            },
            module_depth: 0,
            tracked_names,
        };
        collector.visit_file(file.syntax());
        collector.facts
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn mentions(&self, name: &str) -> bool {
        self.identifiers.iter().any(|(ident, _, _)| ident == name)
            || self.macro_names.contains(name)
    }
}

struct Collector<'a> {
    facts: FileSyntaxFacts,
    module_depth: usize,
    tracked_names: &'a BTreeSet<String>,
}

impl<'ast> Visit<'ast> for Collector<'_> {
    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.module_depth += 1;
        syn::visit::visit_item_mod(self, item);
        self.module_depth -= 1;
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if self.tracked_names.contains(&item.ident.to_string()) {
            self.facts.declarations.insert(located(&item.ident));
        }
        syn::visit::visit_item_struct(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        if self.tracked_names.contains(&item.ident.to_string()) {
            self.facts.declarations.insert(located(&item.ident));
        }
        syn::visit::visit_item_enum(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_attribute(&mut self, item: &'ast syn::Attribute) {
        self.facts.has_cfg |= item.path().is_ident("cfg") || item.path().is_ident("cfg_attr");
        syn::visit::visit_attribute(self, item);
    }

    #[instrument(level = "debug", skip(self, ident))]
    fn visit_ident(&mut self, ident: &'ast proc_macro2::Ident) {
        if self.tracked_names.contains(&ident.to_string()) {
            self.facts.identifiers.insert(located(ident));
        }
    }

    #[instrument(level = "debug", skip(self, path))]
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if self.module_depth == 0 {
            let mut prefix = Vec::new();
            for segment in &path.segments {
                prefix.push(segment.ident.to_string());
                if self.tracked_names.contains(&segment.ident.to_string()) {
                    self.facts
                        .paths
                        .push((located(&segment.ident), prefix.clone()));
                }
            }
        }
        syn::visit::visit_path(self, path);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        collect_token_idents(
            &item.tokens,
            self.tracked_names,
            &mut self.facts.macro_names,
        );
        syn::visit::visit_macro(self, item);
    }
}

#[instrument(level = "debug", skip(ident))]
fn located(ident: &proc_macro2::Ident) -> LocatedName {
    let start = ident.span().start();
    (ident.to_string(), start.line, start.column)
}

#[instrument(level = "debug", skip(tokens, tracked_names, names))]
fn collect_token_idents(
    tokens: &proc_macro2::TokenStream,
    tracked_names: &BTreeSet<String>,
    names: &mut BTreeSet<String>,
) {
    for tree in tokens.clone() {
        match tree {
            proc_macro2::TokenTree::Ident(ident) => {
                if tracked_names.contains(&ident.to_string()) {
                    names.insert(ident.to_string());
                }
            }
            proc_macro2::TokenTree::Group(group) => {
                collect_token_idents(&group.stream(), tracked_names, names)
            }
            _ => {}
        }
    }
}
