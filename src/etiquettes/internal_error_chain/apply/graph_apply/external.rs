//! Check that names used outside a graph's declaration file reach its types.

use std::collections::{BTreeMap, BTreeSet};

use syn::visit::Visit;
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::super::inventory::{CrateTypeIndex, ErrorEdgeTarget};

type LocatedName = (String, usize, usize);

#[instrument(level = "debug", skip(file, index, expected))]
pub(super) fn paths_resolve_to_graph(
    file: &SharedParsedFile,
    index: &CrateTypeIndex,
    expected: &BTreeMap<String, ErrorEdgeTarget>,
) -> bool {
    let mut audit = PathAudit {
        file,
        index,
        expected,
        seen: BTreeSet::new(),
        allowed: BTreeSet::new(),
        module_depth: 0,
        hidden_name: false,
    };
    audit.visit_file(file.syntax());
    !audit.hidden_name && audit.seen.is_subset(&audit.allowed)
}

struct PathAudit<'a> {
    file: &'a SharedParsedFile,
    index: &'a CrateTypeIndex,
    expected: &'a BTreeMap<String, ErrorEdgeTarget>,
    seen: BTreeSet<LocatedName>,
    allowed: BTreeSet<LocatedName>,
    module_depth: usize,
    hidden_name: bool,
}

impl<'ast> Visit<'ast> for PathAudit<'_> {
    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        self.allow_declaration(&item.ident);
        syn::visit::visit_item_struct(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        self.allow_declaration(&item.ident);
        syn::visit::visit_item_enum(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_attribute(&mut self, item: &'ast syn::Attribute) {
        if item.path().is_ident("cfg") || item.path().is_ident("cfg_attr") {
            self.hidden_name = true;
        }
        syn::visit::visit_attribute(self, item);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.module_depth += 1;
        syn::visit::visit_item_mod(self, item);
        self.module_depth -= 1;
    }

    #[instrument(level = "debug", skip(self, ident))]
    fn visit_ident(&mut self, ident: &'ast proc_macro2::Ident) {
        if self.expected.contains_key(&ident.to_string()) {
            let start = ident.span().start();
            self.seen
                .insert((ident.to_string(), start.line, start.column));
        }
    }

    #[instrument(level = "debug", skip(self, path))]
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if self.module_depth == 0 {
            let mut prefix = Vec::new();
            for segment in &path.segments {
                let name = segment.ident.to_string();
                prefix.push(name.clone());
                if let Some(expected) = self.expected.get(&name)
                    && self.index.resolve(self.file, &prefix).as_ref() == Some(expected)
                {
                    let start = segment.ident.span().start();
                    self.allowed.insert((name, start.line, start.column));
                }
            }
        }
        syn::visit::visit_path(self, path);
    }

    #[instrument(level = "debug", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        let mut names = BTreeSet::new();
        super::collect_token_idents(&item.tokens, &mut names);
        self.hidden_name |= names.iter().any(|name| self.expected.contains_key(name));
        syn::visit::visit_macro(self, item);
    }
}

impl PathAudit<'_> {
    #[instrument(level = "trace", skip(self, ident))]
    fn allow_declaration(&mut self, ident: &proc_macro2::Ident) {
        let start = ident.span().start();
        if self.expected.get(&ident.to_string()).is_some_and(|target| {
            target.declaration() == self.file.path()
                && target.line() == start.line
                && target.column() == start.column
        }) {
            self.allowed
                .insert((ident.to_string(), start.line, start.column));
        }
    }
}
