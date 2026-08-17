//! Map inventoried type paths to proof harness coverage status.

use super::ProofHarness;

/// Whether a type has a proof harness test entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestStatus {
    /// `assert_proofs_non_empty::<T>()` call found (with matching type).
    Covered,
    /// Factory impl exists but only a concrete instantiation is tested.
    CoveredConcrete { instantiation: String },
    /// No harness entry found.
    Missing,
}

impl TestStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Covered => "Covered",
            Self::Missing => "Missing",
            Self::CoveredConcrete { instantiation } => {
                if instantiation.is_empty() {
                    "CoveredConcrete"
                } else {
                    "CoveredConcrete"
                }
            }
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Covered => "Covered".to_string(),
            Self::Missing => "Missing".to_string(),
            Self::CoveredConcrete { instantiation } => {
                format!("CoveredConcrete({instantiation})")
            }
        }
    }
}

/// Determine proof and composition harness status for a type path.
pub fn test_status_for_type_path(
    type_path: &str,
    has_factory_impl: bool,
    harness: &ProofHarness,
) -> (TestStatus, TestStatus) {
    let bare_name = type_path.rsplit("::").next().unwrap_or(type_path);

    if harness.non_empty_types.contains(bare_name) {
        return (
            TestStatus::Covered,
            composition_test_status(bare_name, harness),
        );
    }

    if has_factory_impl {
        if let Some(instantiation) = harness
            .non_empty_types
            .iter()
            .find(|t| t.starts_with(bare_name) && t.contains('<'))
            .cloned()
        {
            return (
                TestStatus::CoveredConcrete { instantiation },
                composition_test_status(bare_name, harness),
            );
        }
    }

    let qualified_match = harness
        .non_empty_types
        .iter()
        .any(|t| t == type_path || t.ends_with(&format!("::{bare_name}")));
    if qualified_match {
        return (
            TestStatus::Covered,
            composition_test_status(bare_name, harness),
        );
    }

    (
        TestStatus::Missing,
        composition_test_status(bare_name, harness),
    )
}

fn composition_test_status(name: &str, harness: &ProofHarness) -> TestStatus {
    let found = harness
        .composition_pairs
        .iter()
        .any(|(outer, _inner)| outer.starts_with(name) || outer == name);
    if found {
        TestStatus::Covered
    } else {
        TestStatus::Missing
    }
}
