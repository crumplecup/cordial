//! Loader-owned file-policy evidence, including inactive source syntax.

use crate::loader::SourceLoadView;
use crate::{CordialResult, FileSnapshot};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[cfg(feature = "cli_layout")]
mod cli_layout;
#[cfg(feature = "cli_layout")]
pub use cli_layout::CliLayoutFacts;
#[cfg(feature = "cli_layout")]
pub(crate) use cli_layout::{EntryItem, EntryKind, LayoutCatalog, TypeRec, VariantShape};
#[cfg(feature = "cfg_hygiene")]
mod declarations;
#[cfg(feature = "derives")]
mod derive_facts;
mod native;
#[cfg(feature = "visibility")]
mod visibility;
#[cfg(feature = "derives")]
pub use derive_facts::{
    DeriveFieldFacts, DeriveFieldRead, DeriveFileFacts, DeriveImplFacts, DeriveItemFacts,
    DeriveMethodFacts, DeriveMethodFlags, DeriveModuleFacts, DeriveSetterShape, DeriveStructFacts,
};
#[cfg(feature = "derives")]
pub use native::fixtures::scan_derives_source;
#[cfg(feature = "visibility")]
pub use visibility::{VisibilityFacts, VisibilityKind, VisibilityModule};

#[cfg(feature = "allows")]
pub use native::fixtures::scan_allows_source;
#[cfg(feature = "cfg_hygiene")]
pub use native::fixtures::scan_cfg_hygiene_source;
#[cfg(feature = "cfg_scatter")]
pub use native::fixtures::scan_cfg_scatter_source;
#[cfg(feature = "glob_imports")]
pub use native::fixtures::scan_glob_imports_source;
#[cfg(feature = "inline_tests")]
pub use native::fixtures::scan_inline_tests_source;
#[cfg(feature = "modularity")]
pub use native::fixtures::scan_modularity_source;
#[cfg(feature = "pageantry")]
pub use native::fixtures::scan_pageantry_source;

/// Complete policy collection for a crate's quality scan trees.
pub const ATTR_SOURCE_POLICY_INVENTORY: &str = "source_policy_inventory";

/// Snapshot-bound policy inputs. An empty inventory is explicit evidence;
/// absent or malformed evidence must not establish compliance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, Default)]
pub struct SourcePolicyInventory {
    crate_root: std::path::PathBuf,
    #[cfg(feature = "modularity")]
    modularity: Vec<crate::etiquettes::modularity::ModularitySiteRecord>,
    #[cfg(feature = "pageantry")]
    pageantry: Vec<crate::etiquettes::pageantry::PageantrySourceSite>,
    #[cfg(feature = "derives")]
    derives: Vec<DeriveFileFacts>,
    #[cfg(feature = "visibility")]
    visibility: VisibilityFacts,
    #[cfg(feature = "cli_layout")]
    cli_layout: CliLayoutFacts,
    snapshots: Vec<FileSnapshot>,
    #[cfg(feature = "allows")]
    allows: Vec<crate::etiquettes::allows::AllowSiteRecord>,
    #[cfg(feature = "glob_imports")]
    glob_imports: Vec<crate::etiquettes::glob_imports::GlobImportSiteRecord>,
    #[cfg(feature = "inline_tests")]
    inline_tests: Vec<crate::etiquettes::inline_tests::InlineTestSiteRecord>,
    #[cfg(feature = "cfg_hygiene")]
    cfg_names: Vec<crate::etiquettes::cfg_hygiene::CfgNameOccurrence>,
    #[cfg(feature = "cfg_hygiene")]
    known_cfg_names: std::collections::BTreeSet<String>,
    #[cfg(feature = "cfg_scatter")]
    cfg_groups: Vec<crate::etiquettes::cfg_scatter::CfgScatterGroup>,
}

impl SourcePolicyInventory {
    /// Collect policy facts from the loader-owned native syntax snapshots.
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    #[hotpath::measure(impl_type = "SourcePolicyInventory")]
    pub(crate) fn collect(source: &SourceLoadView, crate_root: &Path) -> CordialResult<Self> {
        let mut inventory = Self {
            crate_root: crate_root.to_path_buf(),
            ..Self::default()
        };
        for tree in crate::loader::quality_scan_trees(crate_root) {
            let is_source = tree == crate_root.join("src");
            if !is_source
                && !cfg!(any(
                    feature = "allows",
                    feature = "glob_imports",
                    feature = "pageantry"
                ))
            {
                continue;
            }
            for entry in walkdir::WalkDir::new(&tree)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
            {
                let path = entry.path();
                if path.extension().is_none_or(|extension| extension != "rs") {
                    continue;
                }
                if crate::loader::path_has_fixtures(path, crate_root)
                    && !(is_source
                        && cfg!(any(
                            feature = "cfg_hygiene",
                            feature = "cfg_scatter",
                            feature = "modularity",
                            feature = "derives"
                        )))
                {
                    continue;
                }
                source.collect_native_file(path, |file| {
                    inventory.collect_native_file(file, crate_root)
                })?;
            }
        }
        #[cfg(feature = "cfg_hygiene")]
        inventory.collect_declarations(crate_root, source.workspace_root(), |path| {
            source.input_text(path)
        })?;
        #[cfg(feature = "visibility")]
        {
            inventory.visibility = Self::collect_visibility(source, crate_root)?;
        }
        #[cfg(feature = "cli_layout")]
        {
            inventory.cli_layout =
                Self::collect_cli_layout(source, crate_root, source.crate_name())?;
        }
        inventory.sort();
        Ok(inventory)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn sort(&mut self) {
        #[cfg(feature = "derives")]
        self.derives.sort_by(|a, b| a.file().cmp(b.file()));
        self.snapshots.sort_by(|a, b| a.path().cmp(b.path()));
        self.snapshots.dedup();
        #[cfg(feature = "modularity")]
        self.modularity.sort_by(|a, b| {
            b.lines()
                .cmp(&a.lines())
                .then(a.kind().as_str().cmp(b.kind().as_str()))
                .then(a.file().cmp(b.file()))
                .then(a.context().cmp(b.context()))
        });
        #[cfg(feature = "pageantry")]
        self.pageantry.sort_by(|a, b| {
            a.site()
                .file()
                .cmp(b.site().file())
                .then(a.site().line().cmp(&b.site().line()))
                .then(a.site().snippet().cmp(b.site().snippet()))
        });
        #[cfg(feature = "glob_imports")]
        self.glob_imports.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.snippet().cmp(b.snippet()))
        });
        #[cfg(feature = "inline_tests")]
        self.inline_tests.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.snippet().cmp(b.snippet()))
        });
        #[cfg(feature = "allows")]
        self.allows.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.context().cmp(b.context()))
                .then(a.snippet().cmp(b.snippet()))
        });
        #[cfg(feature = "cfg_hygiene")]
        self.cfg_names.sort_by(|a, b| {
            a.file()
                .cmp(b.file())
                .then(a.line().cmp(&b.line()))
                .then(a.name().cmp(b.name()))
        });
        #[cfg(feature = "cfg_scatter")]
        self.cfg_groups.sort_by(|a, b| {
            b.non_field_count()
                .cmp(&a.non_field_count())
                .then(a.file().cmp(b.file()))
                .then(a.predicate().cmp(b.predicate()))
        });
    }
}
