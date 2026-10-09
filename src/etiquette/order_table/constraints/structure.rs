//! Rows for the structure areas: inline tests, modularity, cfg scatter and hygiene, visibility, glob imports, pageantry.

use crate::etiquette::order::LintConstraint;

pub(super) const ROWS: &[LintConstraint] = &[
    #[cfg(all(feature = "inline_tests", feature = "tracing"))]
    LintConstraint::new(
        "INLINE-TEST-MOD",
        super::super::ids::TRACING_RULE_IDS,
        super::super::explains::INLINE_TESTS_AFTER_TRACING,
    ),
    #[cfg(all(feature = "inline_tests", feature = "tracing"))]
    LintConstraint::new(
        "INLINE-TEST-CFG",
        super::super::ids::TRACING_RULE_IDS,
        super::super::explains::INLINE_TESTS_AFTER_TRACING,
    ),
    #[cfg(all(feature = "inline_tests", feature = "tracing"))]
    LintConstraint::new(
        "INLINE-TEST-FN",
        super::super::ids::TRACING_RULE_IDS,
        super::super::explains::INLINE_TESTS_AFTER_TRACING,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-FILE",
        super::super::ids::INLINE_TEST_RULE_IDS,
        super::super::explains::MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-FUNCTION",
        super::super::ids::INLINE_TEST_RULE_IDS,
        super::super::explains::MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-TYPES-PER-FILE",
        super::super::ids::INLINE_TEST_RULE_IDS,
        super::super::explains::MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-MODULE-SIZE",
        super::super::ids::INLINE_TEST_RULE_IDS,
        super::super::explains::MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-TOP-HEAVY",
        super::super::ids::INLINE_TEST_RULE_IDS,
        super::super::explains::MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-LOPSIDED",
        super::super::ids::INLINE_TEST_RULE_IDS,
        super::super::explains::MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-COLLAPSE",
        super::super::ids::INLINE_TEST_RULE_IDS,
        super::super::explains::MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "cfg_scatter", feature = "modularity"))]
    LintConstraint::new(
        "CFG-SCATTER-001",
        super::super::ids::MODULARITY_RULE_IDS,
        super::super::explains::CFG_SCATTER_AFTER_MODULARITY,
    ),
    #[cfg(all(feature = "cfg_hygiene", feature = "cfg_scatter"))]
    LintConstraint::new(
        "UNEXPECTED-CFG-001",
        super::super::ids::CFG_SCATTER_RULE_IDS,
        super::super::explains::CFG_HYGIENE_AFTER_CFG_SCATTER,
    ),
    #[cfg(all(feature = "cfg_hygiene", feature = "cfg_scatter"))]
    LintConstraint::new(
        "CFG-VERIFIER-MISMATCH-001",
        super::super::ids::CFG_SCATTER_RULE_IDS,
        super::super::explains::CFG_HYGIENE_AFTER_CFG_SCATTER,
    ),
    #[cfg(all(feature = "visibility", feature = "cfg_hygiene"))]
    LintConstraint::new(
        "VIS-CRATE-FLAT-001",
        super::super::ids::CFG_HYGIENE_RULE_IDS,
        super::super::explains::VISIBILITY_AFTER_CFG_HYGIENE,
    ),
    #[cfg(all(feature = "visibility", feature = "cfg_hygiene"))]
    LintConstraint::new(
        "VIS-MOD-THIN-001",
        super::super::ids::CFG_HYGIENE_RULE_IDS,
        super::super::explains::VISIBILITY_AFTER_CFG_HYGIENE,
    ),
    #[cfg(all(feature = "visibility", feature = "cfg_hygiene"))]
    LintConstraint::new(
        "VIS-MOD-MISMATCH-001",
        super::super::ids::CFG_HYGIENE_RULE_IDS,
        super::super::explains::VISIBILITY_AFTER_CFG_HYGIENE,
    ),
    #[cfg(all(feature = "glob_imports", feature = "visibility"))]
    LintConstraint::new(
        "GLOB-IMPORT-001",
        super::super::ids::VISIBILITY_RULE_IDS,
        super::super::explains::GLOB_IMPORTS_AFTER_VISIBILITY,
    ),
    #[cfg(all(feature = "pageantry", feature = "glob_imports"))]
    LintConstraint::new(
        "PAGEANTRY-TRAIT-001",
        super::super::ids::GLOB_IMPORT_RULE_IDS,
        super::super::explains::PAGEANTRY_AFTER_GLOB_IMPORTS,
    ),
    #[cfg(all(feature = "pageantry", feature = "glob_imports"))]
    LintConstraint::new(
        "PAGEANTRY-BARREL-001",
        super::super::ids::GLOB_IMPORT_RULE_IDS,
        super::super::explains::PAGEANTRY_AFTER_GLOB_IMPORTS,
    ),
    #[cfg(all(feature = "pageantry", feature = "glob_imports"))]
    LintConstraint::new(
        "PAGEANTRY-BARREL-SHIM-001",
        super::super::ids::GLOB_IMPORT_RULE_IDS,
        super::super::explains::PAGEANTRY_AFTER_GLOB_IMPORTS,
    ),
];
