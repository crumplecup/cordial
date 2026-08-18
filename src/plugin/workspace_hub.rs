//! Workspace hub detection — selects coverage profile from workspace members.

use std::collections::HashSet;

use tracing::instrument;

use crate::session::RunFilter;
use crate::targets::discover_crate_targets;

/// Hub crate that selects a coverage profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceHub {
    Elicitation,
    Homecoming,
    Amenable,
    Unknown,
}

impl WorkspaceHub {
    #[instrument(level = "debug", skip(self))]
    pub fn framework_impl_crate(self) -> Option<&'static str> {
        match self {
            Self::Homecoming => Some("homecoming_core"),
            Self::Amenable => Some("amenable_std"),
            Self::Elicitation | Self::Unknown => None,
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub fn framework_patch_set(self) -> Option<&'static str> {
        match self {
            Self::Homecoming => Some("homecoming"),
            Self::Amenable => Some("amenable"),
            Self::Elicitation | Self::Unknown => None,
        }
    }

    #[instrument(level = "debug", skip(self))]
    pub fn framework_primary_trait(self) -> Option<&'static str> {
        match self {
            Self::Homecoming => Some("Code"),
            Self::Amenable | Self::Elicitation | Self::Unknown => None,
        }
    }

    #[instrument(level = "trace", skip(self), ret)]
    pub fn is_framework_std(self) -> bool {
        matches!(self, Self::Homecoming | Self::Amenable)
    }
}

/// Detect hub crate from workspace member names.
#[instrument(level = "debug")]
pub fn detect_workspace_hub(members: &HashSet<String>) -> WorkspaceHub {
    for candidate in ["elicitation", "amenable", "homecoming"] {
        if members.contains(candidate) {
            return match candidate {
                "elicitation" => WorkspaceHub::Elicitation,
                "amenable" => WorkspaceHub::Amenable,
                "homecoming" => WorkspaceHub::Homecoming,
                _ => WorkspaceHub::Unknown,
            };
        }
    }
    WorkspaceHub::Unknown
}

/// Discover workspace hub from cargo metadata at `project_root`.
#[instrument(level = "debug", skip(filter), err(level = "warn"))]
pub fn discover_workspace_hub(
    project_root: &std::path::Path,
    filter: &dyn RunFilter,
) -> crate::error::CordialResult<WorkspaceHub> {
    let members: HashSet<String> = discover_crate_targets(project_root, filter)?
        .into_iter()
        .map(|target| target.crate_name)
        .collect();
    Ok(detect_workspace_hub(&members))
}
