//! Pattern-based inference of std / third-party error types at call sites.

use std::fmt::{Display, Formatter, Result as FmtResult};

use tracing::instrument;
/// Confidence tier for inferred foreign error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForeignTypeConfidence {
    /// High confidence or severity.
    High,
    /// Medium confidence or severity.
    Medium,
}

impl Display for ForeignTypeConfidence {
    #[instrument(level = "trace", skip(self, f))]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::High => write!(f, "FOREIGN-TYPE-CONFIDENCE-HIGH"),
            Self::Medium => write!(f, "FOREIGN-TYPE-CONFIDENCE-MEDIUM"),
        }
    }
}

/// Infer a foreign error type from a source-expression snippet.
#[instrument(level = "debug", skip(source))]
pub fn infer_foreign_error_type(source: &str) -> Option<(String, String, ForeignTypeConfidence)> {
    for rule in FOREIGN_TYPE_RULES {
        if (rule.matches)(source) {
            return Some((
                rule.error_type.to_string(),
                rule.rule_id.to_string(),
                rule.confidence,
            ));
        }
    }
    None
}

struct ForeignTypeRule {
    rule_id: &'static str,
    error_type: &'static str,
    confidence: ForeignTypeConfidence,
    matches: fn(&str) -> bool,
}

const FOREIGN_TYPE_RULES: &[ForeignTypeRule] = &[
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-STD-FMT-001",
        error_type: "std::fmt::Error",
        confidence: ForeignTypeConfidence::High,
        matches: matches_std_fmt,
    },
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-STD-IO-STATUS-001",
        error_type: "std::io::Error",
        confidence: ForeignTypeConfidence::High,
        matches: matches_process_status,
    },
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-STD-IO-FS-001",
        error_type: "std::io::Error",
        confidence: ForeignTypeConfidence::High,
        matches: matches_std_fs,
    },
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-STD-IO-UNQUAL-FS-001",
        error_type: "std::io::Error",
        confidence: ForeignTypeConfidence::High,
        matches: matches_unqualified_fs,
    },
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-STD-IO-STDIO-001",
        error_type: "std::io::Error",
        confidence: ForeignTypeConfidence::High,
        matches: matches_std_io,
    },
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-STD-IO-ENSURE-DIRS-001",
        error_type: "std::io::Error",
        confidence: ForeignTypeConfidence::Medium,
        matches: matches_ensure_dirs,
    },
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-SERDE-JSON-001",
        error_type: "serde_json::Error",
        confidence: ForeignTypeConfidence::High,
        matches: matches_serde_json,
    },
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-CARGO-METADATA-001",
        error_type: "cargo_metadata::Error",
        confidence: ForeignTypeConfidence::High,
        matches: matches_cargo_metadata,
    },
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-CSV-WRITER-001",
        error_type: "csv::Error",
        confidence: ForeignTypeConfidence::High,
        matches: matches_csv_writer,
    },
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-CSV-METHOD-001",
        error_type: "csv::Error",
        confidence: ForeignTypeConfidence::Medium,
        matches: matches_csv_methods,
    },
    ForeignTypeRule {
        rule_id: "FOREIGN-ERROR-TYPE-SYN-PARSE-001",
        error_type: "syn::Error",
        confidence: ForeignTypeConfidence::High,
        matches: matches_syn_parse,
    },
];

#[instrument(level = "debug", skip(source))]
fn matches_std_fmt(source: &str) -> bool {
    source.contains("writeln!(…)") || source.contains("write!(…)")
}

#[instrument(level = "debug", skip(source))]
fn matches_process_status(source: &str) -> bool {
    source.contains(".status(…)")
}

#[instrument(level = "debug", skip(source))]
fn matches_std_fs(source: &str) -> bool {
    source.contains("std::fs::")
}

#[instrument(level = "debug", skip(source))]
fn matches_unqualified_fs(source: &str) -> bool {
    source.starts_with("fs::")
}

#[instrument(level = "debug", skip(source))]
fn matches_std_io(source: &str) -> bool {
    source.contains("std::io::")
}

#[instrument(level = "debug", skip(source))]
fn matches_ensure_dirs(source: &str) -> bool {
    source.contains("ensure_dirs(…)")
}

#[instrument(level = "debug", skip(source))]
fn matches_serde_json(source: &str) -> bool {
    source.contains("serde_json::from_str")
        || source.contains("serde_json::from_slice")
        || source.contains("serde_json::from_reader")
        || source.contains("serde_json::to_string")
        || source.contains("serde_json::to_string_pretty")
        || source.contains("serde_json::to_vec")
        || source.contains("serde_json::to_writer")
}

#[instrument(level = "debug", skip(source))]
fn matches_cargo_metadata(source: &str) -> bool {
    source.contains("cargo_metadata::")
}

#[instrument(level = "debug", skip(source))]
fn matches_csv_writer(source: &str) -> bool {
    source.contains("csv::Writer") || source.contains("csv::Reader") || source.starts_with("csv::")
}

#[instrument(level = "debug", skip(source))]
fn matches_csv_methods(source: &str) -> bool {
    source.contains(".write_record(…)")
        || source.contains(".flush(…)")
        || source.contains(".read_byte_record(…)")
}

#[instrument(level = "debug", skip(source))]
fn matches_syn_parse(source: &str) -> bool {
    source.contains("syn::parse_file")
        || source.contains("syn::parse2")
        || source.contains("syn::parse_str")
}
