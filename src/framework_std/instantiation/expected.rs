//! The instantiations a target expects each generic type to cover.

use std::collections::BTreeMap;

use tracing::instrument;

/// Expected instantiations by generic type, as spelled in the inventory
/// (`chrono::DateTime`): each entry is one tuple of type-argument texts
/// (`["chrono::Utc"]`). Cordial cannot discover a missing instantiation from
/// rustdoc, so the list comes from the target's configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, derive_new::new)]
pub struct ExpectedInstantiations {
    by_generic: BTreeMap<String, Vec<Vec<String>>>,
}

impl ExpectedInstantiations {
    /// The argument tuples expected for `generic_path`; empty when none.
    #[instrument(level = "trace", skip(self))]
    pub fn tuples_for(&self, generic_path: &str) -> &[Vec<String>] {
        self.by_generic
            .get(generic_path)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Whether no generic type has an expected list.
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.by_generic.is_empty()
    }
}
