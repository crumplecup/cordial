//! Reversible getter storage contracts collected into the shared IR.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::config::ErrorFieldStrategy;

use super::Edit;

mod alias;
mod collect;
mod names;
pub(super) mod native;

pub(super) use collect::collect_getter_storage;
pub(super) use native::{has_derive as has_native_derive, known as known_native_derive};

/// Snapshot-bound source contracts; planning supplies crate-wide names and uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GetterContractFacts {
    names: names::Names,
    fields: Vec<GetterFieldContract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GetterFieldContract {
    owner: String,
    field: String,
    storage: GetterContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum GetterContract {
    Owned {
        insertion: Option<GetterInsertion>,
    },
    Generated {
        alias: String,
        edits: Option<Vec<Edit>>,
    },
    Borrowed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GetterInsertion {
    declaration_start: usize,
    field_start: usize,
    cfg: String,
}

impl GetterInsertion {
    #[instrument(level = "trace", skip(self))]
    fn edits(&self, owner: &str, field: &str, name: &str) -> Vec<Edit> {
        vec![
            Edit {
                start: self.declaration_start,
                end: self.declaration_start,
                replacement: format!(
                    "\n/// {}\n{}type {name} = &'static str;\n",
                    alias::marker(owner, field),
                    self.cfg
                ),
            },
            Edit {
                start: self.field_start,
                end: self.field_start,
                replacement: "#[getter(copy)] ".to_string(),
            },
        ]
    }
}

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
