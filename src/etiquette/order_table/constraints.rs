//! Built-in After rows. Validated in `const`.

#[cfg(all(feature = "tracing", feature = "derives"))]
use super::DERIVE_RULE_IDS;
#[cfg(all(feature = "allows", feature = "cli_layout"))]
use super::explains::ALLOW_AFTER_CLI_LAYOUT;
#[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
use super::explains::ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES;
#[cfg(all(feature = "cfg_hygiene", feature = "cfg_scatter"))]
use super::explains::CFG_HYGIENE_AFTER_CFG_SCATTER;
#[cfg(all(feature = "cfg_scatter", feature = "modularity"))]
use super::explains::CFG_SCATTER_AFTER_MODULARITY;
#[cfg(all(feature = "cli_layout", feature = "antipatterns"))]
use super::explains::CLI_LAYOUT_AFTER_ANTIPATTERNS;
#[cfg(all(feature = "crate_attrs", feature = "allows"))]
use super::explains::CRATE_ATTRS_AFTER_ALLOWS;
#[cfg(all(feature = "creusot_diagnostics", feature = "doc_warnings"))]
use super::explains::CREUSOT_AFTER_DOC_WARNINGS;
#[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
use super::explains::DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY;
#[cfg(all(feature = "derives", feature = "crate_attrs"))]
use super::explains::DERIVES_AFTER_CRATE_ATTRS;
#[cfg(all(feature = "doc_warnings", feature = "dependency_freshness"))]
use super::explains::DOC_WARNINGS_AFTER_DEPENDENCY_FRESHNESS;
#[cfg(all(feature = "doc_warnings", feature = "feature_warnings"))]
use super::explains::DOC_WARNINGS_AFTER_FEATURE_WARNINGS;
#[cfg(all(feature = "feature_warnings", feature = "dependency_freshness"))]
use super::explains::FEATURE_WARNINGS_AFTER_DEPENDENCY_FRESHNESS;
#[cfg(all(
    feature = "foreign_error_types",
    any(
        feature = "panics",
        feature = "foreign_error_attenuation",
        feature = "internal_error_chain",
    )
))]
use super::explains::FOREIGN_ERROR_AFTER_ERROR_HANDLING;
#[cfg(all(feature = "glob_imports", feature = "visibility"))]
use super::explains::GLOB_IMPORTS_AFTER_VISIBILITY;
#[cfg(all(feature = "inline_tests", feature = "tracing"))]
use super::explains::INLINE_TESTS_AFTER_TRACING;
#[cfg(all(feature = "modularity", feature = "inline_tests"))]
use super::explains::MODULARITY_AFTER_INLINE_TESTS;
#[cfg(all(feature = "pageantry", feature = "glob_imports"))]
use super::explains::PAGEANTRY_AFTER_GLOB_IMPORTS;
#[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
use super::explains::PROOF_PATTERNS_AFTER_VERUS;
#[cfg(all(feature = "tracing", feature = "derives"))]
use super::explains::TRACING_AFTER_DERIVES;
#[cfg(all(feature = "verus_warnings", feature = "creusot_diagnostics"))]
use super::explains::VERUS_AFTER_CREUSOT;
#[cfg(all(feature = "visibility", feature = "cfg_hygiene"))]
use super::explains::VISIBILITY_AFTER_CFG_HYGIENE;
#[cfg(all(feature = "crate_attrs", feature = "allows"))]
use super::ids::ALLOW_RULE_IDS;
#[cfg(all(feature = "cli_layout", feature = "antipatterns"))]
use super::ids::ANTIPATTERN_RULE_IDS;
#[cfg(all(feature = "visibility", feature = "cfg_hygiene"))]
use super::ids::CFG_HYGIENE_RULE_IDS;
#[cfg(all(feature = "cfg_hygiene", feature = "cfg_scatter"))]
use super::ids::CFG_SCATTER_RULE_IDS;
#[cfg(all(feature = "allows", feature = "cli_layout"))]
use super::ids::CLI_LAYOUT_RULE_IDS;
#[cfg(all(feature = "derives", feature = "crate_attrs"))]
use super::ids::CRATE_ATTR_RULE_IDS;
#[cfg(all(feature = "verus_warnings", feature = "creusot_diagnostics"))]
use super::ids::CREUSOT_RULE_IDS;
#[cfg(any(
    all(feature = "doc_warnings", feature = "dependency_freshness"),
    all(feature = "feature_warnings", feature = "dependency_freshness")
))]
use super::ids::DEPENDENCY_FRESHNESS_RULE_IDS;
#[cfg(all(feature = "creusot_diagnostics", feature = "doc_warnings"))]
use super::ids::DOC_WARNING_RULE_IDS;
#[cfg(all(
    feature = "foreign_error_types",
    any(
        feature = "panics",
        feature = "foreign_error_attenuation",
        feature = "internal_error_chain",
    )
))]
use super::ids::ERROR_HANDLING_RULE_IDS;
#[cfg(all(feature = "doc_warnings", feature = "feature_warnings"))]
use super::ids::FEATURE_WARNING_RULE_IDS;
#[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
use super::ids::FOREIGN_ERROR_TYPE_RULE_IDS;
#[cfg(all(feature = "pageantry", feature = "glob_imports"))]
use super::ids::GLOB_IMPORT_RULE_IDS;
#[cfg(all(feature = "modularity", feature = "inline_tests"))]
use super::ids::INLINE_TEST_RULE_IDS;
use super::ids::KNOWN_IDS;
#[cfg(all(feature = "cfg_scatter", feature = "modularity"))]
use super::ids::MODULARITY_RULE_IDS;
#[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
use super::ids::PAGEANTRY_RULE_IDS;
#[cfg(all(feature = "inline_tests", feature = "tracing"))]
use super::ids::TRACING_RULE_IDS;
#[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
use super::ids::VERUS_WARNING_RULE_IDS;
#[cfg(all(feature = "glob_imports", feature = "visibility"))]
use super::ids::VISIBILITY_RULE_IDS;
use crate::etiquette::order::{LintConstraint, table_is_valid};

pub(super) const CONSTRAINTS: &[LintConstraint] = &[
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-MISSING-INSTRUMENT",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-LEVEL-MISMATCH",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SKIP-MISSING",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-ERR-MISSING",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-ERROR-PATH-SILENT",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-FIELDS-MISSING",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-PROOF-INSTRUMENT",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-UNGATED-INSTRUMENT",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SKIP-INSTRUMENT",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SUBSCRIBER-MAIN",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SUBSCRIBER-TEST",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SUBSCRIBER-LIB",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SUBSCRIBER-RUST-LOG",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SUBSCRIBER-IDEMPOTENT",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-BOUNDARY-MAIN-SILENT",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-STD-PRINTLN",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-STD-EPRINTLN",
        DERIVE_RULE_IDS,
        TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new("TRACING-STD-PRINT", DERIVE_RULE_IDS, TRACING_AFTER_DERIVES),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new("TRACING-STD-EPRINT", DERIVE_RULE_IDS, TRACING_AFTER_DERIVES),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new("TRACING-STD-DBG", DERIVE_RULE_IDS, TRACING_AFTER_DERIVES),
    #[cfg(all(
        feature = "foreign_error_types",
        any(
            feature = "panics",
            feature = "foreign_error_attenuation",
            feature = "internal_error_chain",
        )
    ))]
    LintConstraint::new(
        "FOREIGN-ERROR-CANDIDATE",
        ERROR_HANDLING_RULE_IDS,
        FOREIGN_ERROR_AFTER_ERROR_HANDLING,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-BOX-DYN-ERROR-001",
        FOREIGN_ERROR_TYPE_RULE_IDS,
        ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-STRING-ERROR-001",
        FOREIGN_ERROR_TYPE_RULE_IDS,
        ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001",
        FOREIGN_ERROR_TYPE_RULE_IDS,
        ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-STRUCT-STATIC-REF-001",
        FOREIGN_ERROR_TYPE_RULE_IDS,
        ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001",
        FOREIGN_ERROR_TYPE_RULE_IDS,
        ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-VERSION-IN-MEMBER-001",
        FOREIGN_ERROR_TYPE_RULE_IDS,
        ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "cli_layout", feature = "antipatterns"))]
    LintConstraint::new(
        "CLI-ISLAND-001",
        ANTIPATTERN_RULE_IDS,
        CLI_LAYOUT_AFTER_ANTIPATTERNS,
    ),
    #[cfg(all(feature = "cli_layout", feature = "antipatterns"))]
    LintConstraint::new(
        "CLI-ACT-001",
        ANTIPATTERN_RULE_IDS,
        CLI_LAYOUT_AFTER_ANTIPATTERNS,
    ),
    #[cfg(all(feature = "cli_layout", feature = "antipatterns"))]
    LintConstraint::new(
        "CLI-MAIN-001",
        ANTIPATTERN_RULE_IDS,
        CLI_LAYOUT_AFTER_ANTIPATTERNS,
    ),
    #[cfg(all(feature = "allows", feature = "cli_layout"))]
    LintConstraint::new(
        "ALLOW-ATTR-001",
        CLI_LAYOUT_RULE_IDS,
        ALLOW_AFTER_CLI_LAYOUT,
    ),
    #[cfg(all(feature = "allows", feature = "cli_layout"))]
    LintConstraint::new(
        "ALLOW-VERUS-REASON-001",
        CLI_LAYOUT_RULE_IDS,
        ALLOW_AFTER_CLI_LAYOUT,
    ),
    #[cfg(all(feature = "crate_attrs", feature = "allows"))]
    LintConstraint::new(
        "CRATE-FORBID-UNSAFE-001",
        ALLOW_RULE_IDS,
        CRATE_ATTRS_AFTER_ALLOWS,
    ),
    #[cfg(all(feature = "crate_attrs", feature = "allows"))]
    LintConstraint::new(
        "CRATE-MISSING-DOCS-001",
        ALLOW_RULE_IDS,
        CRATE_ATTRS_AFTER_ALLOWS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-BUILDER-001",
        CRATE_ATTR_RULE_IDS,
        DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-USE-BUILDER-001",
        CRATE_ATTR_RULE_IDS,
        DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-GETTER-001",
        CRATE_ATTR_RULE_IDS,
        DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-SETTER-001",
        CRATE_ATTR_RULE_IDS,
        DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-ASREF-001",
        CRATE_ATTR_RULE_IDS,
        DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-ASSTR-001",
        CRATE_ATTR_RULE_IDS,
        DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-NEW-001",
        CRATE_ATTR_RULE_IDS,
        DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-PUB-FIELD-001",
        CRATE_ATTR_RULE_IDS,
        DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "inline_tests", feature = "tracing"))]
    LintConstraint::new(
        "INLINE-TEST-MOD",
        TRACING_RULE_IDS,
        INLINE_TESTS_AFTER_TRACING,
    ),
    #[cfg(all(feature = "inline_tests", feature = "tracing"))]
    LintConstraint::new(
        "INLINE-TEST-CFG",
        TRACING_RULE_IDS,
        INLINE_TESTS_AFTER_TRACING,
    ),
    #[cfg(all(feature = "inline_tests", feature = "tracing"))]
    LintConstraint::new(
        "INLINE-TEST-FN",
        TRACING_RULE_IDS,
        INLINE_TESTS_AFTER_TRACING,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-FILE",
        INLINE_TEST_RULE_IDS,
        MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-FUNCTION",
        INLINE_TEST_RULE_IDS,
        MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-TYPES-PER-FILE",
        INLINE_TEST_RULE_IDS,
        MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-MODULE-SIZE",
        INLINE_TEST_RULE_IDS,
        MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-TOP-HEAVY",
        INLINE_TEST_RULE_IDS,
        MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-LOPSIDED",
        INLINE_TEST_RULE_IDS,
        MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "modularity", feature = "inline_tests"))]
    LintConstraint::new(
        "MODULARITY-COLLAPSE",
        INLINE_TEST_RULE_IDS,
        MODULARITY_AFTER_INLINE_TESTS,
    ),
    #[cfg(all(feature = "cfg_scatter", feature = "modularity"))]
    LintConstraint::new(
        "CFG-SCATTER-001",
        MODULARITY_RULE_IDS,
        CFG_SCATTER_AFTER_MODULARITY,
    ),
    #[cfg(all(feature = "cfg_hygiene", feature = "cfg_scatter"))]
    LintConstraint::new(
        "UNEXPECTED-CFG-001",
        CFG_SCATTER_RULE_IDS,
        CFG_HYGIENE_AFTER_CFG_SCATTER,
    ),
    #[cfg(all(feature = "cfg_hygiene", feature = "cfg_scatter"))]
    LintConstraint::new(
        "CFG-VERIFIER-MISMATCH-001",
        CFG_SCATTER_RULE_IDS,
        CFG_HYGIENE_AFTER_CFG_SCATTER,
    ),
    #[cfg(all(feature = "visibility", feature = "cfg_hygiene"))]
    LintConstraint::new(
        "VIS-CRATE-FLAT-001",
        CFG_HYGIENE_RULE_IDS,
        VISIBILITY_AFTER_CFG_HYGIENE,
    ),
    #[cfg(all(feature = "visibility", feature = "cfg_hygiene"))]
    LintConstraint::new(
        "VIS-MOD-THIN-001",
        CFG_HYGIENE_RULE_IDS,
        VISIBILITY_AFTER_CFG_HYGIENE,
    ),
    #[cfg(all(feature = "visibility", feature = "cfg_hygiene"))]
    LintConstraint::new(
        "VIS-MOD-MISMATCH-001",
        CFG_HYGIENE_RULE_IDS,
        VISIBILITY_AFTER_CFG_HYGIENE,
    ),
    #[cfg(all(feature = "glob_imports", feature = "visibility"))]
    LintConstraint::new(
        "GLOB-IMPORT-001",
        VISIBILITY_RULE_IDS,
        GLOB_IMPORTS_AFTER_VISIBILITY,
    ),
    #[cfg(all(feature = "pageantry", feature = "glob_imports"))]
    LintConstraint::new(
        "PAGEANTRY-TRAIT-001",
        GLOB_IMPORT_RULE_IDS,
        PAGEANTRY_AFTER_GLOB_IMPORTS,
    ),
    #[cfg(all(feature = "pageantry", feature = "glob_imports"))]
    LintConstraint::new(
        "PAGEANTRY-BARREL-001",
        GLOB_IMPORT_RULE_IDS,
        PAGEANTRY_AFTER_GLOB_IMPORTS,
    ),
    #[cfg(all(feature = "pageantry", feature = "glob_imports"))]
    LintConstraint::new(
        "PAGEANTRY-BARREL-SHIM-001",
        GLOB_IMPORT_RULE_IDS,
        PAGEANTRY_AFTER_GLOB_IMPORTS,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-PATCH",
        PAGEANTRY_RULE_IDS,
        DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MINOR",
        PAGEANTRY_RULE_IDS,
        DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MAJOR",
        PAGEANTRY_RULE_IDS,
        DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MANIFEST-EXACT-PIN",
        PAGEANTRY_RULE_IDS,
        DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MANIFEST-UPPER-BOUND",
        PAGEANTRY_RULE_IDS,
        DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MANIFEST-WILDCARD",
        PAGEANTRY_RULE_IDS,
        DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MANIFEST-TILDE",
        PAGEANTRY_RULE_IDS,
        DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "dependency_freshness", feature = "pageantry"))]
    LintConstraint::new(
        "DEPENDENCY-FRESHNESS-MANIFEST-WORKSPACE-BYPASS",
        PAGEANTRY_RULE_IDS,
        DEPENDENCY_FRESHNESS_AFTER_PAGEANTRY,
    ),
    #[cfg(all(feature = "doc_warnings", feature = "dependency_freshness"))]
    LintConstraint::new(
        "DOC-WARNING-001",
        DEPENDENCY_FRESHNESS_RULE_IDS,
        DOC_WARNINGS_AFTER_DEPENDENCY_FRESHNESS,
    ),
    #[cfg(all(feature = "feature_warnings", feature = "dependency_freshness"))]
    LintConstraint::new(
        "FEATURE-WARNING-001",
        DEPENDENCY_FRESHNESS_RULE_IDS,
        FEATURE_WARNINGS_AFTER_DEPENDENCY_FRESHNESS,
    ),
    #[cfg(all(feature = "feature_warnings", feature = "dependency_freshness"))]
    LintConstraint::new(
        "FEATURE-WARNING-002",
        DEPENDENCY_FRESHNESS_RULE_IDS,
        FEATURE_WARNINGS_AFTER_DEPENDENCY_FRESHNESS,
    ),
    #[cfg(all(feature = "doc_warnings", feature = "feature_warnings"))]
    LintConstraint::new(
        "DOC-WARNING-001",
        FEATURE_WARNING_RULE_IDS,
        DOC_WARNINGS_AFTER_FEATURE_WARNINGS,
    ),
    #[cfg(all(feature = "creusot_diagnostics", feature = "doc_warnings"))]
    LintConstraint::new(
        "CREUSOT-DIAGNOSTIC-001",
        DOC_WARNING_RULE_IDS,
        CREUSOT_AFTER_DOC_WARNINGS,
    ),
    #[cfg(all(feature = "creusot_diagnostics", feature = "doc_warnings"))]
    LintConstraint::new(
        "CREUSOT-DIAGNOSTIC-002",
        DOC_WARNING_RULE_IDS,
        CREUSOT_AFTER_DOC_WARNINGS,
    ),
    #[cfg(all(feature = "verus_warnings", feature = "creusot_diagnostics"))]
    LintConstraint::new("VERUS-WARNING-001", CREUSOT_RULE_IDS, VERUS_AFTER_CREUSOT),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-ASSUME",
        VERUS_WARNING_RULE_IDS,
        PROOF_PATTERNS_AFTER_VERUS,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-ADMIT",
        VERUS_WARNING_RULE_IDS,
        PROOF_PATTERNS_AFTER_VERUS,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-EXTERNAL-BODY",
        VERUS_WARNING_RULE_IDS,
        PROOF_PATTERNS_AFTER_VERUS,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-UNINTERP",
        VERUS_WARNING_RULE_IDS,
        PROOF_PATTERNS_AFTER_VERUS,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-AXIOM",
        VERUS_WARNING_RULE_IDS,
        PROOF_PATTERNS_AFTER_VERUS,
    ),
    #[cfg(all(feature = "proof_patterns", feature = "verus_warnings"))]
    LintConstraint::new(
        "PROOF-PATTERN-BROADCAST",
        VERUS_WARNING_RULE_IDS,
        PROOF_PATTERNS_AFTER_VERUS,
    ),
];

const _: [(); 0] = [(); (!table_is_valid(KNOWN_IDS, CONSTRAINTS)) as usize];
