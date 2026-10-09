//! Rows for the polish areas: dependency freshness, feature warnings, rustdoc, Creusot, Verus, proof patterns.

use crate::etiquette::order::LintConstraint;

pub(super) const ROWS: &[LintConstraint] = &[
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-PATCH",
        super::super::ids::PAGEANTRY_RULE_IDS,
        super::super::explains::DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MINOR",
        super::super::ids::PAGEANTRY_RULE_IDS,
        super::super::explains::DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MAJOR",
        super::super::ids::PAGEANTRY_RULE_IDS,
        super::super::explains::DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MANIFEST-EXACT-PIN",
        super::super::ids::PAGEANTRY_RULE_IDS,
        super::super::explains::DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MANIFEST-UPPER-BOUND",
        super::super::ids::PAGEANTRY_RULE_IDS,
        super::super::explains::DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MANIFEST-WILDCARD",
        super::super::ids::PAGEANTRY_RULE_IDS,
        super::super::explains::DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MANIFEST-TILDE",
        super::super::ids::PAGEANTRY_RULE_IDS,
        super::super::explains::DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MANIFEST-WORKSPACE-BYPASS",
        super::super::ids::PAGEANTRY_RULE_IDS,
        super::super::explains::DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "doc_warnings", feature = "dependency_freshness"))]
    LintConstraint::new(
        "DOC-WARNING-001",
        super::super::ids::DEPENDENCY_FRESHNESS_RULE_IDS,
        super::super::explains::DOC_WARNINGS_AFTER_DEPENDENCY_FRESHNESS,
    ),
    #[cfg(all(feature = "feature_warnings", feature = "dependency_freshness"))]
    LintConstraint::new(
        "FEATURE-WARNING-001",
        super::super::ids::DEPENDENCY_FRESHNESS_RULE_IDS,
        super::super::explains::FEATURE_WARNINGS_AFTER_DEPENDENCY_FRESHNESS,
    ),
    #[cfg(all(feature = "feature_warnings", feature = "dependency_freshness"))]
    LintConstraint::new(
        "FEATURE-WARNING-002",
        super::super::ids::DEPENDENCY_FRESHNESS_RULE_IDS,
        super::super::explains::FEATURE_WARNINGS_AFTER_DEPENDENCY_FRESHNESS,
    ),
    #[cfg(all(feature = "doc_warnings", feature = "feature_warnings"))]
    LintConstraint::new(
        "DOC-WARNING-001",
        super::super::ids::FEATURE_WARNING_RULE_IDS,
        super::super::explains::DOC_WARNINGS_AFTER_FEATURE_WARNINGS,
    ),
    #[cfg(all(feature = "creusot_diagnostics", feature = "doc_warnings"))]
    LintConstraint::new(
        "CREUSOT-DIAGNOSTIC-001",
        super::super::ids::DOC_WARNING_RULE_IDS,
        super::super::explains::CREUSOT_AFTER_DOC_WARNINGS,
    ),
    #[cfg(all(feature = "creusot_diagnostics", feature = "doc_warnings"))]
    LintConstraint::new(
        "CREUSOT-DIAGNOSTIC-002",
        super::super::ids::DOC_WARNING_RULE_IDS,
        super::super::explains::CREUSOT_AFTER_DOC_WARNINGS,
    ),
    #[cfg(all(feature = "verus_warnings", feature = "creusot_diagnostics"))]
    LintConstraint::new(
        "VERUS-WARNING-001",
        super::super::ids::CREUSOT_RULE_IDS,
        super::super::explains::VERUS_AFTER_CREUSOT,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-ASSUME",
        super::super::ids::VERUS_WARNING_RULE_IDS,
        super::super::explains::PROOF_PATTERNS_AFTER_VERUS,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-ADMIT",
        super::super::ids::VERUS_WARNING_RULE_IDS,
        super::super::explains::PROOF_PATTERNS_AFTER_VERUS,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-EXTERNAL-BODY",
        super::super::ids::VERUS_WARNING_RULE_IDS,
        super::super::explains::PROOF_PATTERNS_AFTER_VERUS,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-UNINTERP",
        super::super::ids::VERUS_WARNING_RULE_IDS,
        super::super::explains::PROOF_PATTERNS_AFTER_VERUS,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-AXIOM",
        super::super::ids::VERUS_WARNING_RULE_IDS,
        super::super::explains::PROOF_PATTERNS_AFTER_VERUS,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-BROADCAST",
        super::super::ids::VERUS_WARNING_RULE_IDS,
        super::super::explains::PROOF_PATTERNS_AFTER_VERUS,
    ),
];
