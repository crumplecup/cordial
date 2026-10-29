//! Conservative identifier counts, including opaque macro tokens.

use std::collections::BTreeMap;

use syn::visit::Visit;
use tracing::instrument;

use crate::loader::SharedParsedFile;

#[derive(Default)]
pub(super) struct Names {
    counts: BTreeMap<String, usize>,
}

impl Names {
    #[instrument(level = "debug", skip(files))]
    pub(super) fn collect(files: &[SharedParsedFile]) -> Self {
        let mut names = Self::default();
        for file in files {
            names.visit_file(file.syntax());
        }
        names
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn count(&self, name: &str) -> usize {
        self.counts.get(name).copied().unwrap_or(0)
    }

    #[instrument(level = "trace", skip(self))]
    pub(super) fn reserve(&mut self, owner: &str, field: &str) -> String {
        let suffix: String = field
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().to_string() + chars.as_str())
                    .unwrap_or_default()
            })
            .collect();
        let base = format!("Cordial{}{suffix}Storage", owner.trim_start_matches("r#"));
        let mut name = base.clone();
        let mut index = 2;
        while self.count(&name) != 0 {
            name = format!("{base}{index}");
            index += 1;
        }
        self.counts.insert(name.clone(), 1);
        name
    }

    #[instrument(level = "trace", skip(self, tokens))]
    fn tokens(&mut self, tokens: proc_macro2::TokenStream) {
        for token in tokens {
            match token {
                proc_macro2::TokenTree::Ident(ident) => self.visit_ident(&ident),
                proc_macro2::TokenTree::Group(group) => self.tokens(group.stream()),
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Names {
    #[instrument(level = "trace", skip(self, ident))]
    fn visit_ident(&mut self, ident: &'ast proc_macro2::Ident) {
        *self.counts.entry(ident.to_string()).or_default() += 1;
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        self.tokens(item.tokens.clone());
        syn::visit::visit_path(self, &item.path);
    }

    #[instrument(level = "trace", skip(self, item))]
    fn visit_attribute(&mut self, item: &'ast syn::Attribute) {
        if let syn::Meta::List(list) = &item.meta {
            self.tokens(list.tokens.clone());
        }
        syn::visit::visit_attribute(self, item);
    }
}
