//! Rationale strings for built-in lint-order edges.

#[cfg(all(feature = "tracing", feature = "derives"))]
pub(super) const TRACING_AFTER_DERIVES: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Tracing after derive patterns",
        "Derive replaces hand-rolled constructors and accessors. Instrumenting those functions first adds `#[instrument]` to a surface the next step deletes.",
    );

#[cfg(all(
    feature = "foreign_error_types",
    any(
        feature = "panics",
        feature = "foreign_error_attenuation",
        feature = "internal_error_chain",
    )
))]
pub(super) const FOREIGN_ERROR_AFTER_ERROR_HANDLING: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Foreign error types after error handling",
        "Foreign error types are a more specific error-handling lint. Leaving error handling first is the values signal: fix the general practice, then name the foreign E types that leak onto the Result surface.",
    );

#[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
pub(super) const ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Antipatterns after foreign error types",
        "Antipatterns include specific code smells that often mark structural shortcuts. High signal-to-noise: they usually point at real flaws in the design, so they follow the more specific error-handling pass.",
    );

#[cfg(all(feature = "cli_layout", feature = "antipatterns"))]
pub(super) const CLI_LAYOUT_AFTER_ANTIPATTERNS: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "CLI layout after antipatterns",
        "CLI layout is a more specific subset of antipatterns in the CLI domain: clap types that live only on the binary, skip `act`, or overload `main`. The general smell pass comes first.",
    );

#[cfg(all(feature = "allows", feature = "cli_layout"))]
pub(super) const ALLOW_AFTER_CLI_LAYOUT: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Allow attributes after CLI layout",
        "Allows are another class of antipattern, with high signal-to-noise. They usually need only a small refactor to drop the suppression, so they follow the CLI-domain smell pass.",
    );

#[cfg(all(feature = "crate_attrs", feature = "allows"))]
pub(super) const CRATE_ATTRS_AFTER_ALLOWS: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Crate attributes after allow attributes",
        "Crate-level attributes can catch serious classes of bugs, so they follow the allow pass.",
    );

#[cfg(all(feature = "derives", feature = "crate_attrs"))]
pub(super) const DERIVES_AFTER_CRATE_ATTRS: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Derive patterns after crate attributes",
        "This is the transition from design-error lints to style, taste, and preference. Derives come first in that category because they change the API surface and can change which later lints fire.",
    );

#[cfg(all(feature = "inline_tests", feature = "tracing"))]
pub(super) const INLINE_TESTS_AFTER_TRACING: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Inline tests after tracing",
        "Tracing improves observability and contributes real value, rather than just rearranging the furniture. Move tests after that pass.",
    );

#[cfg(all(feature = "modularity", feature = "inline_tests"))]
pub(super) const MODULARITY_AFTER_INLINE_TESTS: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Modularity after inline tests",
        "Moving tests changes the line count of both files, and that can change which modularity lints fire.",
    );

#[cfg(all(feature = "cfg_scatter", feature = "modularity"))]
pub(super) const CFG_SCATTER_AFTER_MODULARITY: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Cfg scatter after modularity",
        "Cfg scatter is a more restricted modularity exercise, and is often easier after the broader modularity lints are cleared.",
    );

#[cfg(all(feature = "cfg_hygiene", feature = "cfg_scatter"))]
pub(super) const CFG_HYGIENE_AFTER_CFG_SCATTER: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Cfg hygiene after cfg scatter",
        "Lock down cfg hygiene only after you are done moving the flags around.",
    );

#[cfg(all(feature = "visibility", feature = "cfg_hygiene"))]
pub(super) const VISIBILITY_AFTER_CFG_HYGIENE: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Module visibility after cfg hygiene",
        "Resolve the modularity structure before worrying about which types to export and which mods are pub.",
    );

#[cfg(all(feature = "glob_imports", feature = "visibility"))]
pub(super) const GLOB_IMPORTS_AFTER_VISIBILITY: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Glob imports after module visibility",
        "Module structure can still change before visibility is settled. Once exports are resolved, it is safe to enumerate the API in the mod file. Glob imports are often mechanical and help IDE auto-completion.",
    );

#[cfg(all(feature = "pageantry", feature = "glob_imports"))]
pub(super) const PAGEANTRY_AFTER_GLOB_IMPORTS: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Pageantry after glob imports",
        "Pageantry rearranges the furniture to improve readability for human review.",
    );

#[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
pub(super) const DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Dependency freshness after pageantry",
        "No sense upgrading the API until it has stabilized around healthy coding practices, so dependency freshness is one of the last checks.",
    );

#[cfg(all(feature = "feature_warnings", feature = "dependency_freshness"))]
pub(super) const FEATURE_WARNINGS_AFTER_DEPENDENCY_FRESHNESS:
    crate::etiquette::order::OrderExplain = crate::etiquette::order::OrderExplain::new(
    "Feature warnings after dependency freshness",
    "Dependency freshness settles Cargo.toml: the dependency versions and the feature graph. Feature-combination warnings are only meaningful against that settled graph, so they follow it.",
);

#[cfg(all(feature = "doc_warnings", feature = "feature_warnings"))]
pub(super) const DOC_WARNINGS_AFTER_FEATURE_WARNINGS: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Rustdoc warnings after feature warnings",
        "Gating items by feature changes which items exist under which cfg, and so what rustdoc sees. Judge the documentation only once the feature structure has stopped moving.",
    );

#[cfg(all(feature = "doc_warnings", feature = "dependency_freshness"))]
pub(super) const DOC_WARNINGS_AFTER_DEPENDENCY_FRESHNESS: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Rustdoc warnings after dependency freshness",
        "This is the transition to polish. Documentation is the first integrity check at that layer.",
    );

#[cfg(all(feature = "creusot_diagnostics", feature = "doc_warnings"))]
pub(super) const CREUSOT_AFTER_DOC_WARNINGS: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Creusot diagnostics after rustdoc warnings",
        "These warnings do not fire except during verification.",
    );

#[cfg(all(feature = "verus_warnings", feature = "creusot_diagnostics"))]
pub(super) const VERUS_AFTER_CREUSOT: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Verus warnings after Creusot diagnostics",
        "Same logic as Creusot: verifier diagnostics after rustdoc polish. Order between Creusot and Verus is arbitrary.",
    );

#[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
pub(super) const PROOF_PATTERNS_AFTER_VERUS: crate::etiquette::order::OrderExplain =
    crate::etiquette::order::OrderExplain::new(
        "Proof patterns after Verus warnings",
        "These etiquettes are still under development, so they come last in the verifier cluster.",
    );
