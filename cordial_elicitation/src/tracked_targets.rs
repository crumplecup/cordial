// Canonical elicitation upstream ↔ shadow roster (ported from elicit_doc).

/// Workspace members that compose domain APIs rather than mirroring one upstream dependency.
///
/// These are excluded when comparing workspace `elicit_*` members to configured targets.
pub const ELICITATION_INTERFACE_SHADOW_CRATES: &[&str] = &[
    "elicit_db",
    "elicit_ui",
    "elicit_gis",
    "elicit_temporal",
    "elicit_server",
];
/// One upstream crate tracked for both core impl coverage and shadow mirror coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElicitationTrackedTarget {
    /// Upstream dependency name (single source of truth for the target crate version).
    pub upstream: &'static str,
    /// Workspace member that mirrors the upstream public API.
    pub shadow: &'static str,
    /// When true, run impl-dep builds via elicitation's dependency edge for core metrics.
    pub elicitation_impl: bool,
    /// Planned or active `elicitation` Cargo feature for the optional dep.
    pub elicitation_feature: &'static str,
    /// Extra features when documenting the dep from `elicitation` for impl coverage.
    pub impl_dep_features: &'static [&'static str],
}

/// Canonical target list: one upstream crate, core metrics + shadow metrics.
pub const ELICITATION_TRACKED_TARGETS: &[ElicitationTrackedTarget] = &[
    ElicitationTrackedTarget {
        upstream: "accesskit",
        shadow: "elicit_accesskit",
        elicitation_impl: true,
        elicitation_feature: "accesskit",
        impl_dep_features: &["serde", "schemars"],
    },
    ElicitationTrackedTarget {
        upstream: "axum",
        shadow: "elicit_axum",
        elicitation_impl: false,
        elicitation_feature: "axum-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "bevy",
        shadow: "elicit_bevy",
        elicitation_impl: true,
        elicitation_feature: "bevy-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "chrono",
        shadow: "elicit_chrono",
        elicitation_impl: true,
        elicitation_feature: "chrono",
        impl_dep_features: &["serde"],
    },
    ElicitationTrackedTarget {
        upstream: "clap",
        shadow: "elicit_clap",
        elicitation_impl: true,
        elicitation_feature: "clap-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "csv",
        shadow: "elicit_csv",
        elicitation_impl: true,
        elicitation_feature: "csv-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "egui",
        shadow: "elicit_egui",
        elicitation_impl: true,
        elicitation_feature: "egui-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "geo",
        shadow: "elicit_geo",
        elicitation_impl: true,
        elicitation_feature: "geo",
        impl_dep_features: &["use-serde"],
    },
    ElicitationTrackedTarget {
        upstream: "geo-types",
        shadow: "elicit_geo_types",
        elicitation_impl: true,
        elicitation_feature: "geo-types",
        impl_dep_features: &["serde"],
    },
    ElicitationTrackedTarget {
        upstream: "geojson",
        shadow: "elicit_geojson",
        elicitation_impl: true,
        elicitation_feature: "geojson-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "georaster",
        shadow: "elicit_georaster",
        elicitation_impl: true,
        elicitation_feature: "georaster-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "jiff",
        shadow: "elicit_jiff",
        elicitation_impl: true,
        elicitation_feature: "jiff",
        impl_dep_features: &["serde"],
    },
    ElicitationTrackedTarget {
        upstream: "leptos",
        shadow: "elicit_leptos",
        elicitation_impl: false,
        elicitation_feature: "leptos-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "polars",
        shadow: "elicit_polars",
        elicitation_impl: false,
        elicitation_feature: "polars-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "proj",
        shadow: "elicit_proj",
        elicitation_impl: true,
        elicitation_feature: "proj-types",
        impl_dep_features: &["geo-types"],
    },
    ElicitationTrackedTarget {
        upstream: "ratatui",
        shadow: "elicit_ratatui",
        elicitation_impl: true,
        elicitation_feature: "ratatui",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "redb",
        shadow: "elicit_redb",
        elicitation_impl: true,
        elicitation_feature: "redb-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "regex",
        shadow: "elicit_regex",
        elicitation_impl: true,
        elicitation_feature: "regex",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "reqwest",
        shadow: "elicit_reqwest",
        elicitation_impl: true,
        elicitation_feature: "reqwest",
        impl_dep_features: &["json", "cookies", "stream"],
    },
    ElicitationTrackedTarget {
        upstream: "rstar",
        shadow: "elicit_rstar",
        elicitation_impl: true,
        elicitation_feature: "rstar-types",
        impl_dep_features: &["serde"],
    },
    ElicitationTrackedTarget {
        upstream: "serde",
        shadow: "elicit_serde",
        elicitation_impl: true,
        elicitation_feature: "serde",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "serde_json",
        shadow: "elicit_serde_json",
        elicitation_impl: true,
        elicitation_feature: "serde_json",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "sqlx",
        shadow: "elicit_sqlx",
        elicitation_impl: true,
        elicitation_feature: "sqlx-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "surrealdb-types",
        shadow: "elicit_surrealdb",
        elicitation_impl: false,
        elicitation_feature: "surrealdb-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "time",
        shadow: "elicit_time",
        elicitation_impl: true,
        elicitation_feature: "time",
        impl_dep_features: &["serde", "serde-human-readable", "serde-well-known"],
    },
    ElicitationTrackedTarget {
        upstream: "tokio",
        shadow: "elicit_tokio",
        elicitation_impl: true,
        elicitation_feature: "tokio",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "toml",
        shadow: "elicit_toml",
        elicitation_impl: true,
        elicitation_feature: "toml-types",
        impl_dep_features: &["serde"],
    },
    ElicitationTrackedTarget {
        upstream: "tower",
        shadow: "elicit_tower",
        elicitation_impl: true,
        elicitation_feature: "tower-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "uom",
        shadow: "elicit_uom",
        elicitation_impl: false,
        elicitation_feature: "uom-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "url",
        shadow: "elicit_url",
        elicitation_impl: true,
        elicitation_feature: "url",
        impl_dep_features: &["serde"],
    },
    ElicitationTrackedTarget {
        upstream: "uuid",
        shadow: "elicit_uuid",
        elicitation_impl: true,
        elicitation_feature: "uuid",
        impl_dep_features: &["serde"],
    },
    ElicitationTrackedTarget {
        upstream: "wgpu",
        shadow: "elicit_wgpu",
        elicitation_impl: true,
        elicitation_feature: "wgpu-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "winit",
        shadow: "elicit_winit",
        elicitation_impl: true,
        elicitation_feature: "winit-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "wkb",
        shadow: "elicit_wkb",
        elicitation_impl: true,
        elicitation_feature: "wkb-types",
        impl_dep_features: &[],
    },
    ElicitationTrackedTarget {
        upstream: "wkt",
        shadow: "elicit_wkt",
        elicitation_impl: true,
        elicitation_feature: "wkt-types",
        impl_dep_features: &["geo-types", "serde"],
    },
];
