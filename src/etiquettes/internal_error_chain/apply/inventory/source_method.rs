//! Written signature and field uses of an error source method.

use std::collections::BTreeSet;

use syn::spanned::Spanned;
use tracing::instrument;

use crate::loader::SharedParsedFile;

use super::super::span_range;

/// Syntactic facts from an `Error::source()` override.
///
/// The trait path and body uses still require name and flow resolution before
/// they can justify a storage change.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, derive_getters::Getters,
)]
pub struct ErrorSourceMethodEvidence {
    trait_path: Vec<String>,
    return_type_text: Option<String>,
    self_field_uses: BTreeSet<String>,
}

impl ErrorSourceMethodEvidence {
    #[instrument(level = "debug", skip(file, method, trait_path))]
    pub(super) fn from_method(
        file: &SharedParsedFile,
        method: &syn::ImplItemFn,
        trait_path: &syn::Path,
    ) -> Self {
        let return_type_text = match &method.sig.output {
            syn::ReturnType::Type(_, ty) => span_range(file.source(), ty.span())
                .map(|(start, end)| file.source()[start..end].to_string()),
            syn::ReturnType::Default => None,
        };
        Self {
            trait_path: trait_path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect(),
            return_type_text,
            self_field_uses: BTreeSet::new(),
        }
    }

    #[instrument(level = "trace", skip(self, name))]
    pub(super) fn record_self_field(&mut self, name: &syn::Ident) {
        self.self_field_uses.insert(name.to_string());
    }
}
