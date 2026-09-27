//! Rationale strings for built-in lint-order edges.

use crate::etiquette::order::OrderExplain;

pub(super) const TRACING_AFTER_DERIVES: OrderExplain = OrderExplain::new(
    "Tracing after derive patterns",
    "Derive replaces hand-rolled constructors and accessors. Instrumenting those functions first adds `#[instrument]` to a surface the next step deletes.",
);

pub(super) const FOREIGN_ERROR_AFTER_ERROR_HANDLING: OrderExplain = OrderExplain::new(
    "Foreign error types after error handling",
    "Foreign error types are a more specific error-handling lint. Leaving error handling first is the values signal: fix the general practice, then name the foreign E types that leak onto the Result surface.",
);

pub(super) const ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES: OrderExplain = OrderExplain::new(
    "Antipatterns after foreign error types",
    "Antipatterns include specific code smells that often mark structural shortcuts. High signal-to-noise: they usually point at real flaws in the design, so they follow the more specific error-handling pass.",
);

pub(super) const CLI_LAYOUT_AFTER_ANTIPATTERNS: OrderExplain = OrderExplain::new(
    "CLI layout after antipatterns",
    "CLI layout is a more specific subset of antipatterns in the CLI domain: clap types that live only on the binary, skip `act`, or overload `main`. The general smell pass comes first.",
);

pub(super) const ALLOW_AFTER_CLI_LAYOUT: OrderExplain = OrderExplain::new(
    "Allow attributes after CLI layout",
    "Allows are another class of antipattern, with high signal-to-noise. They usually need only a small refactor to drop the suppression, so they follow the CLI-domain smell pass.",
);

pub(super) const CRATE_ATTRS_AFTER_ALLOWS: OrderExplain = OrderExplain::new(
    "Crate attributes after allow attributes",
    "Crate-level attributes can catch serious classes of bugs, so they follow the allow pass.",
);

pub(super) const DERIVES_AFTER_CRATE_ATTRS: OrderExplain = OrderExplain::new(
    "Derive patterns after crate attributes",
    "This is the transition from design-error lints to style, taste, and preference. Derives come first in that category because they change the API surface and can change which later lints fire.",
);

pub(super) const INLINE_TESTS_AFTER_TRACING: OrderExplain = OrderExplain::new(
    "Inline tests after tracing",
    "Tracing improves observability and contributes real value, rather than just rearranging the furniture. Move tests after that pass.",
);

pub(super) const MODULARITY_AFTER_INLINE_TESTS: OrderExplain = OrderExplain::new(
    "Modularity after inline tests",
    "Moving tests changes the line count of both files, and that can change which modularity lints fire.",
);

pub(super) const CFG_SCATTER_AFTER_MODULARITY: OrderExplain = OrderExplain::new(
    "Cfg scatter after modularity",
    "Cfg scatter is a more restricted modularity exercise, and is often easier after the broader modularity lints are cleared.",
);

pub(super) const CFG_HYGIENE_AFTER_CFG_SCATTER: OrderExplain = OrderExplain::new(
    "Cfg hygiene after cfg scatter",
    "Lock down cfg hygiene only after you are done moving the flags around.",
);

pub(super) const VISIBILITY_AFTER_CFG_HYGIENE: OrderExplain = OrderExplain::new(
    "Module visibility after cfg hygiene",
    "Resolve the modularity structure before worrying about which types to export and which mods are pub.",
);

pub(super) const GLOB_IMPORTS_AFTER_VISIBILITY: OrderExplain = OrderExplain::new(
    "Glob imports after module visibility",
    "Module structure can still change before visibility is settled. Once exports are resolved, it is safe to enumerate the API in the mod file. Glob imports are often mechanical and help IDE auto-completion.",
);

pub(super) const PAGEANTRY_AFTER_GLOB_IMPORTS: OrderExplain = OrderExplain::new(
    "Pageantry after glob imports",
    "Pageantry rearranges the furniture to improve readability for human review.",
);

pub(super) const DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY: OrderExplain = OrderExplain::new(
    "Dependency freshness after pageantry",
    "No sense upgrading the API until it has stabilized around healthy coding practices, so dependency freshness is one of the last checks.",
);

pub(super) const DOC_WARNINGS_AFTER_DEPENDENCY_FRESHNESS: OrderExplain = OrderExplain::new(
    "Rustdoc warnings after dependency freshness",
    "This is the transition to polish. Documentation is the first integrity check at that layer.",
);

pub(super) const CREUSOT_AFTER_DOC_WARNINGS: OrderExplain = OrderExplain::new(
    "Creusot diagnostics after rustdoc warnings",
    "These warnings do not fire except during verification.",
);

pub(super) const VERUS_AFTER_CREUSOT: OrderExplain = OrderExplain::new(
    "Verus warnings after Creusot diagnostics",
    "Same logic as Creusot: verifier diagnostics after rustdoc polish. Order between Creusot and Verus is arbitrary.",
);

pub(super) const PROOF_PATTERNS_AFTER_VERUS: OrderExplain = OrderExplain::new(
    "Proof patterns after Verus warnings",
    "These etiquettes are still under development, so they come last in the verifier cluster.",
);
