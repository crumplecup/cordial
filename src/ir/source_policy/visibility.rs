//! Backend-independent module-tree evidence for visibility policy.
mod collect;
use crate::{CordialResult, FileSnapshot, loader::SourceLoadView};
pub(crate) use collect::collect;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

impl super::SourcePolicyInventory {
    #[tracing::instrument(level = "trace", skip(self, facts))]
    pub(crate) fn set_visibility(&mut self, root: &Path, facts: VisibilityFacts) {
        self.crate_root = root.to_path_buf();
        self.visibility = facts;
    }

    /// Collect module-tree evidence using the shared loader's source snapshots.
    #[tracing::instrument(level = "debug", skip(source), err(level = "warn"))]
    pub fn collect_visibility(
        source: &SourceLoadView,
        root: &Path,
    ) -> CordialResult<VisibilityFacts> {
        collect(root, |path| {
            let file = source.parsed_file(path, "visibility-inventory")?;
            Ok((
                super::compatibility::collect_visibility(&file)?,
                FileSnapshot::new(file.path().to_path_buf(), file.source_digest()),
            ))
        })
    }
}

/// Complete module hierarchy and the source snapshots that establish it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters, Default)]
pub struct VisibilityFacts {
    root: Option<VisibilityModule>,
    snapshots: Vec<FileSnapshot>,
}

/// A module's declared visibility and direct public-name counts before thresholds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_getters::Getters)]
pub struct VisibilityModule {
    path: String,
    file: PathBuf,
    #[getter(copy)]
    line: u32,
    #[getter(copy)]
    declared_vis: VisibilityKind,
    #[getter(copy)]
    parent_declared_pub: bool,
    #[getter(copy)]
    ancestors_all_pub: bool,
    #[getter(copy)]
    is_crate_root: bool,
    #[getter(copy)]
    leaf_pub: usize,
    #[getter(copy)]
    leaf_crate: usize,
    children: Vec<VisibilityModule>,
}

/// Visibility categories retained by the existing module policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisibilityKind {
    /// Inherited visibility.
    Private,
    /// Unrestricted public visibility.
    Pub,
    /// Crate visibility, including other restricted paths under existing policy.
    PubCrate,
    /// Parent-module visibility.
    PubSuper,
}
impl VisibilityKind {
    /// Render the policy's visibility label.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Pub => "pub",
            Self::PubCrate => "pub(crate)",
            Self::PubSuper => "pub(super)",
        }
    }
    /// Whether a module can contribute to an unrestricted public path.
    #[tracing::instrument(level = "trace", skip(self), ret)]
    pub fn is_unrestricted_pub(self) -> bool {
        matches!(self, Self::Pub)
    }
}

// Transient loader assembly inputs; consumers only receive the resolved tree above.
#[derive(Default, derive_new::new)]
pub(crate) struct VisibilityBody {
    leaf_pub: usize,
    leaf_crate: usize,
    children: Vec<VisibilityChild>,
}
#[derive(derive_new::new)]
pub(crate) struct VisibilityChild {
    name: String,
    line: u32,
    visibility: VisibilityKind,
    body: VisibilityChildBody,
}
pub(crate) enum VisibilityChildBody {
    Inline(VisibilityBody),
    External(Option<String>),
}
