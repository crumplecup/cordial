//! Reversible getter storage contracts collected into the shared IR.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::Edit;

mod alias;
mod collect;
mod derives;
mod names;

pub(super) use collect::collect_getter_storage;
pub(super) use derives::known_derive;

pub(super) type GetterFields = BTreeMap<(String, String), GetterStorageFacts>;
pub(super) type GetterFiles = BTreeMap<PathBuf, GetterFields>;

/// Alias identity and the complete attribute/declaration edits for one field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub(super) struct GetterStorageFacts {
    alias: String,
    #[getter(copy)]
    target: ErrorFieldStrategy,
    edits: Option<Vec<Edit>>,
}

impl GetterStorageFacts {
    #[instrument(level = "trace", skip(self, strategy))]
    pub(super) fn plan(&self, strategy: ErrorFieldStrategy) -> Option<(&str, &[Edit])> {
        (self.target == strategy).then_some((
            if strategy == ErrorFieldStrategy::Borrowed {
                self.alias.as_str()
            } else {
                "String"
            },
            self.edits.as_deref()?,
        ))
    }
}
