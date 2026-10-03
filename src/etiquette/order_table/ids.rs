//! Rule-id slices for the built-in lint-order table.

/// Derive-pattern rule ids. Tracing runs after this set.
pub const DERIVE_RULE_IDS: &[&str] = &[
    #[cfg(feature = "derives")]
    "DERIVE-BUILDER-001",
    #[cfg(feature = "derives")]
    "DERIVE-USE-BUILDER-001",
    #[cfg(feature = "derives")]
    "DERIVE-GETTER-001",
    #[cfg(feature = "derives")]
    "DERIVE-SETTER-001",
    #[cfg(feature = "derives")]
    "DERIVE-ASREF-001",
    #[cfg(feature = "derives")]
    "DERIVE-ASSTR-001",
    #[cfg(feature = "derives")]
    "DERIVE-NEW-001",
    #[cfg(feature = "derives")]
    "DERIVE-PUB-FIELD-001",
];

pub(super) const TRACING_RULE_IDS: &[&str] = &[
    #[cfg(feature = "tracing")]
    "TRACING-MISSING-INSTRUMENT",
    #[cfg(feature = "tracing")]
    "TRACING-LEVEL-MISMATCH",
    #[cfg(feature = "tracing")]
    "TRACING-SKIP-MISSING",
    #[cfg(feature = "tracing")]
    "TRACING-ERR-MISSING",
    #[cfg(feature = "tracing")]
    "TRACING-ERROR-PATH-SILENT",
    #[cfg(feature = "tracing")]
    "TRACING-FIELDS-MISSING",
    #[cfg(feature = "tracing")]
    "TRACING-PROOF-INSTRUMENT",
    #[cfg(feature = "tracing")]
    "TRACING-UNGATED-INSTRUMENT",
    #[cfg(feature = "tracing")]
    "TRACING-SKIP-INSTRUMENT",
    #[cfg(feature = "tracing")]
    "TRACING-SUBSCRIBER-MAIN",
    #[cfg(feature = "tracing")]
    "TRACING-SUBSCRIBER-TEST",
    #[cfg(feature = "tracing")]
    "TRACING-SUBSCRIBER-LIB",
    #[cfg(feature = "tracing")]
    "TRACING-SUBSCRIBER-RUST-LOG",
    #[cfg(feature = "tracing")]
    "TRACING-SUBSCRIBER-IDEMPOTENT",
    #[cfg(feature = "tracing")]
    "TRACING-BOUNDARY-MAIN-SILENT",
    #[cfg(feature = "tracing")]
    "TRACING-STD-PRINTLN",
    #[cfg(feature = "tracing")]
    "TRACING-STD-EPRINTLN",
    #[cfg(feature = "tracing")]
    "TRACING-STD-PRINT",
    #[cfg(feature = "tracing")]
    "TRACING-STD-EPRINT",
    #[cfg(feature = "tracing")]
    "TRACING-STD-DBG",
];

/// Rule ids that feed the hand-composed Error handling quality-report area.
///
/// Box-dyn / string-error antipatterns also roll into that area, but they
/// belong to the Antipatterns etiquette. Naming them here would cycle once
/// Antipatterns runs after foreign error types.
pub(crate) const ERROR_HANDLING_RULE_IDS: &[&str] = &[
    #[cfg(feature = "panics")]
    "PANIC-SOURCE-PANIC",
    #[cfg(feature = "panics")]
    "PANIC-SOURCE-UNREACHABLE",
    #[cfg(feature = "panics")]
    "PANIC-SOURCE-EXPECT",
    #[cfg(feature = "panics")]
    "PANIC-SOURCE-UNWRAP",
    #[cfg(feature = "panics")]
    "PANIC-SOURCE-COMPILE-ERROR",
    #[cfg(feature = "foreign_error_attenuation")]
    "ERROR-HANDLING-CHAIN-PRESERVED",
    #[cfg(feature = "foreign_error_attenuation")]
    "ERROR-HANDLING-CHAIN-BREAK",
    #[cfg(feature = "foreign_error_attenuation")]
    "ERROR-HANDLING-PENDING-INFRA",
    #[cfg(feature = "foreign_error_attenuation")]
    "ERROR-HANDLING-NEUTRAL",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-COMPLIANCE-STRINGIFY-001",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-COMPLIANCE-DISCARD-TYPED-001",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-COMPLIANCE-SOURCE-SHAPE-001",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-COMPLIANCE-SOURCE-TRACK-CALLER-001",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-COMPLIANCE-ARCH-PARENT-001",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-BOX-001",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-COMPLIANCE-ARCH-KIND-VARIANT-001",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-COMPLIANCE-ARCH-ORPHAN-SOURCE-001",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-INTERNAL-LEAF-001",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-INTERNAL-LINK-001",
    #[cfg(feature = "internal_error_chain")]
    "ERROR-CHAIN-INTERNAL-NESTED-001",
];

pub(super) const FOREIGN_ERROR_TYPE_RULE_IDS: &[&str] = &[
    #[cfg(feature = "foreign_error_types")]
    "FOREIGN-ERROR-CANDIDATE",
];

pub(super) const ANTIPATTERN_RULE_IDS: &[&str] = &[
    #[cfg(feature = "antipatterns")]
    "ANTIPATTERN-BOX-DYN-ERROR-001",
    #[cfg(feature = "antipatterns")]
    "ANTIPATTERN-STRING-ERROR-001",
    #[cfg(feature = "antipatterns")]
    "ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001",
    #[cfg(feature = "antipatterns")]
    "ANTIPATTERN-STRUCT-STATIC-REF-001",
    #[cfg(feature = "antipatterns")]
    "ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001",
    #[cfg(feature = "antipatterns")]
    "ANTIPATTERN-VERSION-IN-MEMBER-001",
];

pub(super) const CLI_LAYOUT_RULE_IDS: &[&str] = &[
    #[cfg(feature = "cli_layout")]
    "CLI-ISLAND-001",
    #[cfg(feature = "cli_layout")]
    "CLI-ACT-001",
    #[cfg(feature = "cli_layout")]
    "CLI-MAIN-001",
];

pub(super) const ALLOW_RULE_IDS: &[&str] = &[
    #[cfg(feature = "allows")]
    "ALLOW-ATTR-001",
    #[cfg(feature = "allows")]
    "ALLOW-VERUS-REASON-001",
];

pub(super) const CRATE_ATTR_RULE_IDS: &[&str] = &[
    #[cfg(feature = "crate_attrs")]
    "CRATE-FORBID-UNSAFE-001",
    #[cfg(feature = "crate_attrs")]
    "CRATE-MISSING-DOCS-001",
];

pub(super) const INLINE_TEST_RULE_IDS: &[&str] = &[
    #[cfg(feature = "inline_tests")]
    "INLINE-TEST-MOD",
    #[cfg(feature = "inline_tests")]
    "INLINE-TEST-CFG",
    #[cfg(feature = "inline_tests")]
    "INLINE-TEST-FN",
];

pub(super) const MODULARITY_RULE_IDS: &[&str] = &[
    #[cfg(feature = "modularity")]
    "MODULARITY-FILE",
    #[cfg(feature = "modularity")]
    "MODULARITY-FUNCTION",
    #[cfg(feature = "modularity")]
    "MODULARITY-TYPES-PER-FILE",
    #[cfg(feature = "modularity")]
    "MODULARITY-MODULE-SIZE",
    #[cfg(feature = "modularity")]
    "MODULARITY-TOP-HEAVY",
    #[cfg(feature = "modularity")]
    "MODULARITY-LOPSIDED",
    #[cfg(feature = "modularity")]
    "MODULARITY-COLLAPSE",
];

pub(super) const CFG_SCATTER_RULE_IDS: &[&str] = &[
    #[cfg(feature = "cfg_scatter")]
    "CFG-SCATTER-001",
];

pub(super) const CFG_HYGIENE_RULE_IDS: &[&str] = &[
    #[cfg(feature = "cfg_hygiene")]
    "UNEXPECTED-CFG-001",
    #[cfg(feature = "cfg_hygiene")]
    "CFG-VERIFIER-MISMATCH-001",
];

pub(super) const VISIBILITY_RULE_IDS: &[&str] = &[
    #[cfg(feature = "visibility")]
    "VIS-CRATE-FLAT-001",
    #[cfg(feature = "visibility")]
    "VIS-MOD-THIN-001",
    #[cfg(feature = "visibility")]
    "VIS-MOD-MISMATCH-001",
];

pub(super) const GLOB_IMPORT_RULE_IDS: &[&str] = &[
    #[cfg(feature = "glob_imports")]
    "GLOB-IMPORT-001",
];

pub(super) const PAGEANTRY_RULE_IDS: &[&str] = &[
    #[cfg(feature = "pageantry")]
    "PAGEANTRY-TRAIT-001",
    #[cfg(feature = "pageantry")]
    "PAGEANTRY-BARREL-001",
    #[cfg(feature = "pageantry")]
    "PAGEANTRY-BARREL-SHIM-001",
];

pub(super) const DEPENDENCY_FRESHNESS_RULE_IDS: &[&str] = &[
    #[cfg(feature = "dependency_freshness")]
    "DEPENDENCY-FRESHNESS-PATCH",
    #[cfg(feature = "dependency_freshness")]
    "DEPENDENCY-FRESHNESS-MINOR",
    #[cfg(feature = "dependency_freshness")]
    "DEPENDENCY-FRESHNESS-MAJOR",
    #[cfg(feature = "dependency_freshness")]
    "DEPENDENCY-FRESHNESS-MANIFEST-EXACT-PIN",
    #[cfg(feature = "dependency_freshness")]
    "DEPENDENCY-FRESHNESS-MANIFEST-UPPER-BOUND",
    #[cfg(feature = "dependency_freshness")]
    "DEPENDENCY-FRESHNESS-MANIFEST-WILDCARD",
    #[cfg(feature = "dependency_freshness")]
    "DEPENDENCY-FRESHNESS-MANIFEST-TILDE",
    #[cfg(feature = "dependency_freshness")]
    "DEPENDENCY-FRESHNESS-MANIFEST-WORKSPACE-BYPASS",
];

pub(super) const DOC_WARNING_RULE_IDS: &[&str] = &[
    #[cfg(feature = "doc_warnings")]
    "DOC-WARNING-001",
];

pub(super) const CREUSOT_RULE_IDS: &[&str] = &[
    #[cfg(feature = "creusot_diagnostics")]
    "CREUSOT-DIAGNOSTIC-001",
    #[cfg(feature = "creusot_diagnostics")]
    "CREUSOT-DIAGNOSTIC-002",
];

pub(super) const VERUS_WARNING_RULE_IDS: &[&str] = &[
    #[cfg(feature = "verus_warnings")]
    "VERUS-WARNING-001",
];

pub(super) const PROOF_PATTERN_RULE_IDS: &[&str] = &[
    #[cfg(feature = "proof_patterns")]
    "PROOF-PATTERN-ASSUME",
    #[cfg(feature = "proof_patterns")]
    "PROOF-PATTERN-ADMIT",
    #[cfg(feature = "proof_patterns")]
    "PROOF-PATTERN-EXTERNAL-BODY",
    #[cfg(feature = "proof_patterns")]
    "PROOF-PATTERN-UNINTERP",
    #[cfg(feature = "proof_patterns")]
    "PROOF-PATTERN-AXIOM",
    #[cfg(feature = "proof_patterns")]
    "PROOF-PATTERN-BROADCAST",
];

pub(super) const KNOWN_IDS: &[&str] = &{
    let mut ids = [""; DERIVE_RULE_IDS.len()
        + TRACING_RULE_IDS.len()
        + ERROR_HANDLING_RULE_IDS.len()
        + FOREIGN_ERROR_TYPE_RULE_IDS.len()
        + ANTIPATTERN_RULE_IDS.len()
        + CLI_LAYOUT_RULE_IDS.len()
        + ALLOW_RULE_IDS.len()
        + CRATE_ATTR_RULE_IDS.len()
        + INLINE_TEST_RULE_IDS.len()
        + MODULARITY_RULE_IDS.len()
        + CFG_SCATTER_RULE_IDS.len()
        + CFG_HYGIENE_RULE_IDS.len()
        + VISIBILITY_RULE_IDS.len()
        + GLOB_IMPORT_RULE_IDS.len()
        + PAGEANTRY_RULE_IDS.len()
        + DEPENDENCY_FRESHNESS_RULE_IDS.len()
        + DOC_WARNING_RULE_IDS.len()
        + CREUSOT_RULE_IDS.len()
        + VERUS_WARNING_RULE_IDS.len()
        + PROOF_PATTERN_RULE_IDS.len()];
    let mut index = 0;
    index = append_ids(&mut ids, index, DERIVE_RULE_IDS);
    index = append_ids(&mut ids, index, TRACING_RULE_IDS);
    index = append_ids(&mut ids, index, ERROR_HANDLING_RULE_IDS);
    index = append_ids(&mut ids, index, FOREIGN_ERROR_TYPE_RULE_IDS);
    index = append_ids(&mut ids, index, ANTIPATTERN_RULE_IDS);
    index = append_ids(&mut ids, index, CLI_LAYOUT_RULE_IDS);
    index = append_ids(&mut ids, index, ALLOW_RULE_IDS);
    index = append_ids(&mut ids, index, CRATE_ATTR_RULE_IDS);
    index = append_ids(&mut ids, index, INLINE_TEST_RULE_IDS);
    index = append_ids(&mut ids, index, MODULARITY_RULE_IDS);
    index = append_ids(&mut ids, index, CFG_SCATTER_RULE_IDS);
    index = append_ids(&mut ids, index, CFG_HYGIENE_RULE_IDS);
    index = append_ids(&mut ids, index, VISIBILITY_RULE_IDS);
    index = append_ids(&mut ids, index, GLOB_IMPORT_RULE_IDS);
    index = append_ids(&mut ids, index, PAGEANTRY_RULE_IDS);
    index = append_ids(&mut ids, index, DEPENDENCY_FRESHNESS_RULE_IDS);
    index = append_ids(&mut ids, index, DOC_WARNING_RULE_IDS);
    index = append_ids(&mut ids, index, CREUSOT_RULE_IDS);
    index = append_ids(&mut ids, index, VERUS_WARNING_RULE_IDS);
    append_ids(&mut ids, index, PROOF_PATTERN_RULE_IDS);
    ids
};

const fn append_ids(dest: &mut [&'static str], start: usize, src: &[&'static str]) -> usize {
    let mut offset = 0;
    while offset < src.len() {
        dest[start + offset] = src[offset];
        offset += 1;
    }
    start + src.len()
}
