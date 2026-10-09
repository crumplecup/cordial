//! The instantiations a target asks to track for each generic type.

use std::collections::BTreeMap;

use tracing::instrument;

/// How many derived combinations are allowed before a person must list the
/// ones to track.
const DEFAULT_DERIVE_CAP: usize = 16;

/// Instantiations configured per generic type, as spelled in the inventory
/// (`chrono::DateTime`): each entry is one tuple of type-argument texts
/// (`["chrono::Utc"]`).
///
/// Configuration is an override, not a requirement. A generic type with no
/// entry gets its instantiations derived from rustdoc (the implementors of
/// each parameter's declared bounds). An entry replaces the derivation, and an
/// empty entry turns it off for that type.
#[derive(Debug, Clone, PartialEq, Eq, derive_getters::Getters, derive_setters::Setters)]
#[setters(prefix = "with_")]
pub struct ExpectedInstantiations {
    #[getter(skip)]
    #[setters(skip)]
    by_generic: BTreeMap<String, Vec<Vec<String>>>,
    /// The most combinations a derivation may produce for one type.
    #[getter(copy)]
    derive_cap: usize,
}

impl Default for ExpectedInstantiations {
    #[instrument(level = "debug")]
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}

impl ExpectedInstantiations {
    /// Configured tuples by generic type, with the default derivation cap.
    #[instrument(level = "trace", skip(by_generic))]
    pub fn new(by_generic: BTreeMap<String, Vec<Vec<String>>>) -> Self {
        Self {
            by_generic,
            derive_cap: DEFAULT_DERIVE_CAP,
        }
    }

    /// The tuples configured for `generic_path`; `None` when the type has no
    /// entry, which means "derive them".
    #[instrument(level = "trace", skip(self))]
    pub fn configured(&self, generic_path: &str) -> Option<&[Vec<String>]> {
        self.by_generic.get(generic_path).map(Vec::as_slice)
    }
}
