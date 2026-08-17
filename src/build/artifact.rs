use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Metadata for one `cargo rustdoc` invocation (elicit_doc-compatible shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildArtifact {
    pub cache_key: String,
    pub crate_name: String,
    pub build_kind: BuildKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_member: Option<String>,
    pub features: Vec<String>,
    pub uses_default_features: bool,
    pub rustdoc_json: PathBuf,
    pub built_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<DocFingerprint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildKind {
    WorkspaceMember,
    /// Upstream crate rustdoc built with features activated via a shadow member dependency.
    MemberDependency,
    SysrootLibrary,
}

/// Fingerprint recorded after a successful rustdoc build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocFingerprint {
    pub rustdoc_sha256: String,
    pub crate_version: String,
}

impl BuildArtifact {
    pub fn workspace_member(crate_name: impl Into<String>, rustdoc_json: PathBuf) -> Self {
        let crate_name = crate_name.into();
        Self {
            cache_key: crate_name.clone(),
            crate_name,
            build_kind: BuildKind::WorkspaceMember,
            reference_member: None,
            features: Vec::new(),
            uses_default_features: true,
            rustdoc_json,
            built_at: chrono_like_timestamp(),
            fingerprint: None,
        }
    }

    pub fn sysroot_library(crate_name: impl Into<String>, rustdoc_json: PathBuf) -> Self {
        let crate_name = crate_name.into();
        Self {
            cache_key: format!("impl-dep-{crate_name}"),
            crate_name,
            build_kind: BuildKind::SysrootLibrary,
            reference_member: None,
            features: Vec::new(),
            uses_default_features: true,
            rustdoc_json,
            built_at: chrono_like_timestamp(),
            fingerprint: None,
        }
    }

    pub fn shadow_dep(
        shadow_crate: impl Into<String>,
        upstream_crate: impl Into<String>,
        rustdoc_json: PathBuf,
        features: Vec<String>,
        uses_default_features: bool,
    ) -> Self {
        let shadow_crate = shadow_crate.into();
        let upstream_crate = upstream_crate.into();
        let cache_key =
            crate::store::StoreLayout::shadow_dep_cache_stem(&shadow_crate, &upstream_crate);
        Self {
            cache_key,
            crate_name: upstream_crate,
            build_kind: BuildKind::MemberDependency,
            reference_member: Some(shadow_crate),
            features,
            uses_default_features,
            rustdoc_json,
            built_at: chrono_like_timestamp(),
            fingerprint: None,
        }
    }
}

fn chrono_like_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}
