//! Written signature and field uses of an error source method.

use std::collections::BTreeSet;

use tracing::instrument;

/// Syntactic facts from an `Error::source()` override.
///
/// The trait path and body uses still require name and flow resolution before
/// they can justify a storage change.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    derive_getters::Getters,
    derive_new::new,
)]
pub struct ErrorSourceMethodEvidence {
    trait_path: Vec<String>,
    return_type_text: Option<String>,
    self_field_uses: BTreeSet<String>,
}

impl ErrorSourceMethodEvidence {
    #[instrument(level = "trace", skip(self, name))]
    pub(super) fn record_field_name(&mut self, name: String) {
        self.self_field_uses.insert(name);
    }
}
