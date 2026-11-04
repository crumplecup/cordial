//! Snapshot-bound CLI layout evidence from the loader's canonical source trees.
mod catalog;
mod compatibility;
mod native;
use crate::{CordialResult, FileSnapshot, loader::SourceLoadView};
pub(crate) use catalog::{LayoutCatalog, TypeRec, VariantShape};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Resolved types, dispatch targets and binary entry shapes before policy evaluation.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct CliLayoutFacts {
    catalog: catalog::LayoutCatalog,
    entries: Vec<EntryItem>,
    snapshots: Vec<FileSnapshot>,
}
impl Default for CliLayoutFacts {
    #[tracing::instrument(level = "trace")]
    fn default() -> Self {
        Self::new(
            catalog::LayoutCatalog::new(String::new()),
            Vec::new(),
            Vec::new(),
        )
    }
}
/// One top-level binary declaration; macro payloads remain opaque.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, derive_new::new,
)]
pub struct EntryItem {
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    name: String,
    #[getter(copy)]
    kind: EntryKind,
}
/// Entry syntax relevant to the thin-main contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    /// Process entry function and its observed operations.
    Main {
        has_match: bool,
        has_parse: bool,
        has_act: bool,
    },
    /// Additional function.
    Function,
    /// Module declaration.
    Module,
    /// Struct declaration.
    Struct,
    /// Enum declaration.
    Enum,
    /// Implementation block.
    Impl,
}
impl super::SourcePolicyInventory {
    /// Collect through the shipping loader's shared syntax boundary.
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    pub fn collect_cli_layout(
        source: &SourceLoadView,
        root: &Path,
        name: &str,
    ) -> CordialResult<CliLayoutFacts> {
        compatibility::collect(source, root, name)
    }
    #[tracing::instrument(level = "trace", skip(self, facts))]
    pub(crate) fn set_cli_layout(&mut self, root: &Path, facts: CliLayoutFacts) {
        self.crate_root = root.to_path_buf();
        self.cli_layout = facts;
    }
}
