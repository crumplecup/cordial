//! Rows for the code-shape areas: tracing after derives, then error types, antipatterns, CLI layout, allows, crate attributes, derives.

use crate::etiquette::order::LintConstraint;

pub(super) const ROWS: &[LintConstraint] = &[
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-MISSING-INSTRUMENT",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-LEVEL-MISMATCH",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SKIP-MISSING",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-ERR-MISSING",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-ERROR-PATH-SILENT",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-FIELDS-MISSING",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-PROOF-INSTRUMENT",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-UNGATED-INSTRUMENT",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SKIP-INSTRUMENT",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SUBSCRIBER-MAIN",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SUBSCRIBER-TEST",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SUBSCRIBER-LIB",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SUBSCRIBER-RUST-LOG",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-SUBSCRIBER-IDEMPOTENT",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-BOUNDARY-MAIN-SILENT",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-STD-PRINTLN",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-STD-EPRINTLN",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-STD-PRINT",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-STD-EPRINT",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
    #[cfg(all(feature = "tracing", feature = "derives"))]
    LintConstraint::new(
        "TRACING-STD-DBG",
        super::super::ids::DERIVE_RULE_IDS,
        super::super::explains::TRACING_AFTER_DERIVES,
    ),
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
        super::super::ids::ERROR_HANDLING_RULE_IDS,
        super::super::explains::FOREIGN_ERROR_AFTER_ERROR_HANDLING,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-BOX-DYN-ERROR-001",
        super::super::ids::FOREIGN_ERROR_TYPE_RULE_IDS,
        super::super::explains::ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-STRING-ERROR-001",
        super::super::ids::FOREIGN_ERROR_TYPE_RULE_IDS,
        super::super::explains::ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001",
        super::super::ids::FOREIGN_ERROR_TYPE_RULE_IDS,
        super::super::explains::ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-STRUCT-STATIC-REF-001",
        super::super::ids::FOREIGN_ERROR_TYPE_RULE_IDS,
        super::super::explains::ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-UNNAMED-CONTRACT-BOUND-001",
        super::super::ids::FOREIGN_ERROR_TYPE_RULE_IDS,
        super::super::explains::ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "antipatterns", feature = "foreign_error_types"))]
    LintConstraint::new(
        "ANTIPATTERN-VERSION-IN-MEMBER-001",
        super::super::ids::FOREIGN_ERROR_TYPE_RULE_IDS,
        super::super::explains::ANTIPATTERNS_AFTER_FOREIGN_ERROR_TYPES,
    ),
    #[cfg(all(feature = "cli_layout", feature = "antipatterns"))]
    LintConstraint::new(
        "CLI-ISLAND-001",
        super::super::ids::ANTIPATTERN_RULE_IDS,
        super::super::explains::CLI_LAYOUT_AFTER_ANTIPATTERNS,
    ),
    #[cfg(all(feature = "cli_layout", feature = "antipatterns"))]
    LintConstraint::new(
        "CLI-ACT-001",
        super::super::ids::ANTIPATTERN_RULE_IDS,
        super::super::explains::CLI_LAYOUT_AFTER_ANTIPATTERNS,
    ),
    #[cfg(all(feature = "cli_layout", feature = "antipatterns"))]
    LintConstraint::new(
        "CLI-MAIN-001",
        super::super::ids::ANTIPATTERN_RULE_IDS,
        super::super::explains::CLI_LAYOUT_AFTER_ANTIPATTERNS,
    ),
    #[cfg(all(feature = "allows", feature = "cli_layout"))]
    LintConstraint::new(
        "ALLOW-ATTR-001",
        super::super::ids::CLI_LAYOUT_RULE_IDS,
        super::super::explains::ALLOW_AFTER_CLI_LAYOUT,
    ),
    #[cfg(all(feature = "allows", feature = "cli_layout"))]
    LintConstraint::new(
        "ALLOW-VERUS-REASON-001",
        super::super::ids::CLI_LAYOUT_RULE_IDS,
        super::super::explains::ALLOW_AFTER_CLI_LAYOUT,
    ),
    #[cfg(all(feature = "crate_attrs", feature = "allows"))]
    LintConstraint::new(
        "CRATE-FORBID-UNSAFE-001",
        super::super::ids::ALLOW_RULE_IDS,
        super::super::explains::CRATE_ATTRS_AFTER_ALLOWS,
    ),
    #[cfg(all(feature = "crate_attrs", feature = "allows"))]
    LintConstraint::new(
        "CRATE-MISSING-DOCS-001",
        super::super::ids::ALLOW_RULE_IDS,
        super::super::explains::CRATE_ATTRS_AFTER_ALLOWS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-BUILDER-001",
        super::super::ids::CRATE_ATTR_RULE_IDS,
        super::super::explains::DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-USE-BUILDER-001",
        super::super::ids::CRATE_ATTR_RULE_IDS,
        super::super::explains::DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-GETTER-001",
        super::super::ids::CRATE_ATTR_RULE_IDS,
        super::super::explains::DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-SETTER-001",
        super::super::ids::CRATE_ATTR_RULE_IDS,
        super::super::explains::DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-ASREF-001",
        super::super::ids::CRATE_ATTR_RULE_IDS,
        super::super::explains::DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-ASSTR-001",
        super::super::ids::CRATE_ATTR_RULE_IDS,
        super::super::explains::DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-NEW-001",
        super::super::ids::CRATE_ATTR_RULE_IDS,
        super::super::explains::DERIVES_AFTER_CRATE_ATTRS,
    ),
    #[cfg(all(feature = "derives", feature = "crate_attrs"))]
    LintConstraint::new(
        "DERIVE-PUB-FIELD-001",
        super::super::ids::CRATE_ATTR_RULE_IDS,
        super::super::explains::DERIVES_AFTER_CRATE_ATTRS,
    ),
];
