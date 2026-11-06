//! Conservative identifier counts, including opaque macro tokens.

use std::collections::BTreeMap;

use quote::ToTokens;
use tracing::instrument;

use crate::loader::SharedParsedFile;

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Names {
    counts: BTreeMap<String, usize>,
}

impl Names {
    #[instrument(level = "trace", skip(self))]
    pub(super) fn record(&mut self, name: &str) {
        *self.counts.entry(name.to_string()).or_default() += 1;
    }
    #[instrument(level = "debug", skip(file))]
    pub(super) fn collect(file: &SharedParsedFile) -> Self {
        let mut names = Self::default();
        names.tokens(file.syntax().to_token_stream());
        names
    }

    #[instrument(level = "trace", skip(self, other))]
    pub(super) fn extend(&mut self, other: &Self) {
        for (name, count) in &other.counts {
            *self.counts.entry(name.clone()).or_default() += count;
        }
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
                proc_macro2::TokenTree::Ident(ident) => self.record(&ident.to_string()),
                proc_macro2::TokenTree::Group(group) => self.tokens(group.stream()),
                _ => {}
            }
        }
    }
}
