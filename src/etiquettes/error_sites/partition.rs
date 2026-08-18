//! Heuristic partition of error sites into internal vs foreign origins.

use super::types::{ErrorOriginClass, ErrorSiteKind, ErrorSiteScanRow};

use tracing::instrument;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionedErrorSiteRow {
    pub crate_name: String,
    pub kind: ErrorSiteKind,
    pub context: String,
    pub file: std::path::PathBuf,
    pub line: u32,
    pub source_snippet: String,
    pub site_snippet: String,
    pub origin_class: ErrorOriginClass,
    pub origin_detail: String,
    pub rationale: String,
}

#[instrument(level = "debug")]
pub fn partition_error_site_records(
    records: &[ErrorSiteScanRow],
    crate_name: &str,
) -> Vec<PartitionedErrorSiteRow> {
    records
        .iter()
        .map(|record| partition_error_site_row(record, crate_name))
        .collect()
}

#[instrument(level = "debug")]
pub fn partition_error_site_row(
    finding: &ErrorSiteScanRow,
    crate_name: &str,
) -> PartitionedErrorSiteRow {
    let (origin_class, origin_detail, rationale) = classify_origin(finding, crate_name);
    PartitionedErrorSiteRow {
        crate_name: finding.crate_name.clone(),
        kind: finding.kind,
        context: finding.context.clone(),
        file: finding.file.clone(),
        line: finding.line,
        source_snippet: finding.source_snippet.clone(),
        site_snippet: finding.site_snippet.clone(),
        origin_class,
        origin_detail,
        rationale,
    }
}

fn classify_origin(
    finding: &ErrorSiteScanRow,
    crate_name: &str,
) -> (ErrorOriginClass, String, String) {
    let source = finding.source_snippet.as_str();
    let site = finding.site_snippet.as_str();

    if mentions_crate_error(source, site) {
        return (
            ErrorOriginClass::Internal,
            "CordialError".to_string(),
            "explicit crate error constructor".to_string(),
        );
    }

    if is_internal_propagation(source, site, crate_name) {
        return (
            ErrorOriginClass::Internal,
            crate_name.to_string(),
            "same-crate error propagation or Option-to-domain-error conversion".to_string(),
        );
    }

    if is_opaque_snippet(source) {
        return (
            ErrorOriginClass::Edge,
            "opaque".to_string(),
            "source expression not resolved by syn scan".to_string(),
        );
    }

    match finding.kind {
        ErrorSiteKind::ReturnErr => {
            if source.contains("CordialError") {
                (
                    ErrorOriginClass::Internal,
                    "CordialError".to_string(),
                    "return Err with crate error constructor".to_string(),
                )
            } else {
                (
                    ErrorOriginClass::Edge,
                    "unknown-return".to_string(),
                    "return Err payload not resolved from snippet".to_string(),
                )
            }
        }
        ErrorSiteKind::OkOr => (
            ErrorOriginClass::Internal,
            "CordialError".to_string(),
            "Option::ok_or constructs crate error in this codebase".to_string(),
        ),
        ErrorSiteKind::QuestionMark if source.contains(".map_err") => (
            ErrorOriginClass::Internal,
            "CordialResult".to_string(),
            "propagation after map_err conversion".to_string(),
        ),
        ErrorSiteKind::MapErr => {
            classify_call_source(source, crate_name, "foreign Result before map_err")
        }
        ErrorSiteKind::QuestionMark | ErrorSiteKind::IfLetErr | ErrorSiteKind::MatchErr => {
            classify_call_source(source, crate_name, "Result-bearing expression")
        }
    }
}

fn classify_call_source(
    source: &str,
    crate_name: &str,
    context_label: &str,
) -> (ErrorOriginClass, String, String) {
    if source.starts_with("writeln!(…)")
        || source.starts_with("write!(…)")
        || source.contains("writeln!(…)")
        || source.contains("write!(…)")
    {
        return (
            ErrorOriginClass::Other,
            "std".to_string(),
            format!("{context_label}; std formatting macro"),
        );
    }

    if source.contains("std::") {
        let detail = extract_path_root(source).unwrap_or_else(|| "std".to_string());
        return (
            ErrorOriginClass::Other,
            detail,
            format!("{context_label}; std library path"),
        );
    }

    if source.contains("crate::") || source.contains(&format!("{crate_name}::")) {
        return (
            ErrorOriginClass::Internal,
            crate_name.to_string(),
            format!("{context_label}; same-crate path"),
        );
    }

    if is_bare_local_call(source) {
        return (
            ErrorOriginClass::Internal,
            "crate-local".to_string(),
            format!("{context_label}; unqualified fn call"),
        );
    }

    if let Some(root) = extract_path_root(source) {
        if matches!(root.as_str(), "std" | "core" | "alloc") {
            return (
                ErrorOriginClass::Other,
                root,
                format!("{context_label}; standard library path"),
            );
        }
        return (
            ErrorOriginClass::Other,
            root,
            format!("{context_label}; third-party crate path"),
        );
    }

    if source.contains('.') {
        return (
            ErrorOriginClass::Edge,
            "method-call".to_string(),
            format!("{context_label}; receiver type not resolved"),
        );
    }

    (
        ErrorOriginClass::Edge,
        "unknown".to_string(),
        format!("{context_label}; could not classify from snippet"),
    )
}

fn mentions_crate_error(source: &str, site: &str) -> bool {
    for snippet in [source, site] {
        if snippet.contains("CordialError") {
            return true;
        }
        for constructor in [
            "::invariant(",
            "::cargo_metadata(",
            "::syn_parse(",
            "::json_parse(",
        ] {
            if snippet.contains(constructor) {
                return true;
            }
        }
    }
    false
}

fn is_internal_propagation(source: &str, site: &str, crate_name: &str) -> bool {
    if site.contains("ok_or(") || site.contains("ok_or_else(") {
        return true;
    }
    if source.contains("ok_or(") || source.contains("ok_or_else(") {
        return true;
    }
    if source.contains("crate::") || source.contains(&format!("{crate_name}::")) {
        return true;
    }
    false
}

fn is_opaque_snippet(source: &str) -> bool {
    source == "…" || source.starts_with("…")
}

fn is_bare_local_call(source: &str) -> bool {
    let Some(head) = source.split('(').next() else {
        return false;
    };
    let head = head.trim();
    if head.contains("::") || head.contains('.') || head.ends_with('!') {
        return false;
    }
    head.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase() || ch == '_')
}

fn extract_path_root(source: &str) -> Option<String> {
    let before_paren = source.split('(').next()?.trim();
    let path_prefix = before_paren.split('.').next()?.trim();
    let root_end = path_prefix.find("::")?;
    Some(path_prefix[..root_end].to_string())
}
