//! Loader-owned file-policy evidence, including inactive source syntax.

use crate::FileSnapshot;
use serde::{Deserialize, Serialize};

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
