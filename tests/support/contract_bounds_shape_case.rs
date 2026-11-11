//! One row of the contract-bounds shape matrix, and the check that runs a list of rows.
#![cfg(feature = "antipatterns")]

use cordial::ContractRecordDump;
use miette::{IntoDiagnostic, WrapErr};

#[path = "contract_bounds_fixtures.rs"]
mod contract_bounds_fixtures;

use contract_bounds_fixtures::fixtures_root;

// ---------------------------------------------------------------------
// Shape matrix
//
// See `docs/planning/contract-bounds-shape-matrix.md`. Instead of one
// hand-written `#[test]` per syntactic shape the scanner needs to
// recognize, `SHAPE_CASES` enumerates `(verifier, shape, expected
// outcome)` rows and one driving test dispatches each row to the right
// `scan_*_contract_bounds_source` entry point. A row with
// `expect_flagged: true` for a shape that *should* be silenced (a real,
// correctly-named call) documents a known scanner gap rather than hiding
// it -- grep for `expect_flagged: true` to find every open case.
// ---------------------------------------------------------------------

/// A contract-bounds scanner entry point.
pub type Scanner = fn(
    &str,
    &std::path::Path,
    &std::path::Path,
    &[ContractRecordDump],
) -> cordial::CordialResult<Vec<cordial::AntipatternSiteRecord>>;

pub struct ShapeCase {
    /// Names the pattern under test; appears in the panic message so a
    /// broken row points straight at its own shape.
    pub id: &'static str,
    /// The scanner for this row's verifier.
    pub scan: Scanner,
    /// Documentary only -- which clause list the fixture's single clause
    /// lives in. Not passed to the scanner (which detects this from the
    /// source itself); read this alongside `id` when auditing coverage.
    pub kind: &'static str,
    pub source: &'static str,
    pub registry: fn() -> miette::Result<Vec<ContractRecordDump>>,
    /// `false` = the scanner must stay silent (a real, correctly-named
    /// call). `true` = the scanner is expected to flag this shape --
    /// either because it's genuinely a raw/unnamed bound, or because it's
    /// a documented gap (see `docs/planning/contract-bounds-shape-matrix.md`).
    pub expect_flagged: bool,
}

/// Scan every case with its verifier's scanner and compare against what it expects.
pub fn check_cases(cases: &[ShapeCase]) -> miette::Result<()> {
    let src_root = fixtures_root();
    for case in cases {
        let path = src_root.join(format!("shape_{}.rs", case.id));
        let registry = (case.registry)()?;
        let findings = (case.scan)(case.source, &path, &src_root, &registry)
            .into_diagnostic()
            .wrap_err_with(|| format!("scan shape case {} ({})", case.id, case.kind))?;
        let flagged = !findings.is_empty();
        assert_eq!(
            flagged, case.expect_flagged,
            "shape case {:?} ({}): expected flagged={}, got flagged={} (findings: {findings:?})",
            case.id, case.kind, case.expect_flagged, flagged,
        );
    }
    Ok(())
}
