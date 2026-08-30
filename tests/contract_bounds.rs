#![cfg(feature = "antipatterns")]

use std::fs;
use std::path::PathBuf;

use cordial::{
    ANTIPATTERNS_ETIQUETTE, AntipatternRuleId, ContractRecordDump, RunAll, Session, SessionBuilder,
    scan_crate_contract_bounds, scan_creusot_contract_bounds_source,
    scan_kani_contract_bounds_source, scan_verus_contract_bounds_source,
};

use miette::{IntoDiagnostic, WrapErr};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/quality/antipatterns")
}

fn fixture(name: &str) -> miette::Result<String> {
    fs::read_to_string(fixtures_root().join(name))
        .into_diagnostic()
        .wrap_err_with(|| format!("read fixture {name}"))
}

fn logic_fn_record(verifier: &str, kind: &str, fn_name: &str) -> ContractRecordDump {
    ContractRecordDump {
        evidence: format!("fixture::{fn_name}"),
        verifier: verifier.to_string(),
        kind: kind.to_string(),
        fragment: format!("#[logic(open)]\nfn {fn_name}(c: char) -> bool {{\n    true\n}}"),
    }
}

fn kani_type_record(kind: &str, evidence: &str) -> ContractRecordDump {
    ContractRecordDump {
        evidence: evidence.to_string(),
        verifier: "kani".to_string(),
        kind: kind.to_string(),
        fragment: "value >= 0".to_string(),
    }
}

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

#[derive(Debug, Clone, Copy)]
enum Verifier {
    Kani,
    Creusot,
    Verus,
}

struct ShapeCase {
    /// Names the pattern under test; appears in the panic message so a
    /// broken row points straight at its own shape.
    id: &'static str,
    verifier: Verifier,
    /// Documentary only -- which clause list the fixture's single clause
    /// lives in. Not passed to the scanner (which detects this from the
    /// source itself); read this alongside `id` when auditing coverage.
    kind: &'static str,
    source: &'static str,
    registry: fn() -> Vec<ContractRecordDump>,
    /// `false` = the scanner must stay silent (a real, correctly-named
    /// call). `true` = the scanner is expected to flag this shape --
    /// either because it's genuinely a raw/unnamed bound, or because it's
    /// a documented gap (see `docs/planning/contract-bounds-shape-matrix.md`).
    expect_flagged: bool,
}

fn empty_registry() -> Vec<ContractRecordDump> {
    Vec::new()
}

fn abbreviated_ensures_registry() -> Vec<ContractRecordDump> {
    vec![ContractRecordDump {
        evidence: "amenable_std::rust_std::RustStdStandard<i32>".to_string(),
        verifier: "kani".to_string(),
        kind: "ensures".to_string(),
        fragment: "value >= 0".to_string(),
    }]
}

fn nested_generic_cell_registry() -> Vec<ContractRecordDump> {
    vec![ContractRecordDump {
        evidence: "amenable_std::rust_std::RustStdStandard<Cell<i32>>".to_string(),
        verifier: "kani".to_string(),
        kind: "ensures".to_string(),
        fragment: "actual == expected".to_string(),
    }]
}

fn nonnegative_ensures_registry() -> Vec<ContractRecordDump> {
    vec![kani_type_record("ensures", "fixture::NonNegative")]
}

fn write_stores_new_value_registry() -> Vec<ContractRecordDump> {
    vec![logic_fn_record(
        "verus",
        "ensures",
        "write_stores_new_value",
    )]
}

fn bytes_lifetime_registry() -> Vec<ContractRecordDump> {
    vec![kani_type_record(
        "ensures",
        "amenable_std::rust_std::RustStdStandard<std::str::Bytes<'static>>",
    )]
}

fn into_iter_const_generic_registry() -> Vec<ContractRecordDump> {
    vec![kani_type_record(
        "ensures",
        "amenable_std::rust_std::RustStdStandard<std::array::IntoIter<i32, 3>>",
    )]
}

fn pair_like_comma_generic_registry() -> Vec<ContractRecordDump> {
    vec![kani_type_record(
        "ensures",
        "amenable_std::rust_std::RustStdStandard<PairLike<i32, i32>>",
    )]
}

const SHAPE_CASES: &[ShapeCase] = &[
    ShapeCase {
        id: "creusot_trivial_bare_result",
        verifier: Verifier::Creusot,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    creusot, VERIFY_SOMETHING_SRC, {
        #[trusted]
        #[ensures(result)]
        fn verify_something() -> bool {
            match 1 {
                1 => true,
                _ => false,
            }
        }
    }
}
"#,
        registry: empty_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "verus_trivial_bare_result",
        verifier: Verifier::Verus,
        kind: "ensures",
        source: r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_something() -> (result: bool)
    ensures
        result,
{
    true
}

} // verus!
"#,
        registry: empty_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "verus_named_call_with_final_argument",
        verifier: Verifier::Verus,
        kind: "ensures",
        source: r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

pub struct VerusCellModel {
    value: i32,
}

verus! {

impl VerusCellModel {
    pub fn set(&mut self, new_value: i32)
        ensures
            write_stores_new_value(new_value as int, final(self).value as int),
    {
        self.value = new_value;
    }
}

} // verus!
"#,
        registry: write_stores_new_value_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "kani_type_suffix_abbreviated_call_site",
        verifier: Verifier::Kani,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    kani, VERIFY_ABBREVIATED_SRC, {
        #[kani::proof]
        fn verify_abbreviated() {
            let value: i32 = kani::any();
            assert!(RustStdStandard::<i32>::ensures(value), "message");
        }
    }
}
"#,
        registry: abbreviated_ensures_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "kani_type_suffix_nested_generic_evidence",
        verifier: Verifier::Kani,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    kani, VERIFY_NESTED_GENERIC_SRC, {
        #[kani::proof]
        fn verify_nested_generic() {
            let cell = std::cell::Cell::new(0i32);
            assert!(RustStdStandard::<Cell<i32>>::ensures((cell.get(), 0)), "message");
        }
    }
}
"#,
        registry: nested_generic_cell_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "kani_negated_call_registered_type",
        verifier: Verifier::Kani,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    kani, VERIFY_REJECTS_ZERO_SRC, {
        #[kani::proof]
        fn verify_rejects_zero() {
            let value: i32 = kani::any();
            assert!(!fixture::NonNegative::ensures(value), "message");
        }
    }
}
"#,
        registry: nonnegative_ensures_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "kani_double_negated_call",
        verifier: Verifier::Kani,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    kani, VERIFY_DOUBLE_NEGATED_SRC, {
        #[kani::proof]
        fn verify_double_negated() {
            let value: i32 = kani::any();
            assert!(!!fixture::NonNegative::ensures(value), "message");
        }
    }
}
"#,
        registry: nonnegative_ensures_registry,
        // A double negation isn't folded by the matcher, so it's still
        // flagged -- genuinely, not a documented gap: `!!name(x)` isn't
        // the same recognized shape as `name(x)` or `!name(x)`.
        expect_flagged: true,
    },
    ShapeCase {
        id: "kani_fully_qualified_qself_call",
        verifier: Verifier::Kani,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    kani, VERIFY_QUALIFIED_SRC, {
        #[kani::proof]
        fn verify_qualified() {
            let value: i32 = kani::any();
            assert!(
                <fixture::NonNegative as Ensures<KaniVerifier>>::ensures(value),
                "message"
            );
        }
    }
}
"#,
        registry: nonnegative_ensures_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "kani_assert_eq_call_shape_synthesis",
        verifier: Verifier::Kani,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    kani, VERIFY_STILL_RAW_SRC, {
        #[kani::proof]
        fn verify_still_raw() {
            let value: i32 = kani::any();
            assert_eq!(fixture::NonNegative::round_trip(value), value);
        }
    }
}
"#,
        registry: nonnegative_ensures_registry,
        // `assert_eq!(A, B)` is always synthesized as the raw equation
        // `A == B`, never matched against a registered call -- even
        // though `fixture::NonNegative` is registered, it's `round_trip`
        // being called, not `ensures`/`requires`, so this is genuinely
        // unnamed, not a gap.
        expect_flagged: true,
    },
    ShapeCase {
        id: "verus_negated_bare_result",
        verifier: Verifier::Verus,
        kind: "ensures",
        source: r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_something() -> (result: bool)
    ensures
        !result,
{
    false
}

} // verus!
"#,
        registry: empty_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "kani_turbofish_nested_generic_lifetime",
        verifier: Verifier::Kani,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    kani, VERIFY_BYTES_SRC, {
        #[kani::proof]
        fn verify_bytes_yields_the_utf8_encoding() {
            let byte: u8 = kani::any();
            let s = (byte as char).to_string();
            let mut it = s.bytes();
            assert!(
                RustStdStandard::<std::str::Bytes<'static>>::ensures((it.next(), Some(byte))),
                "message"
            );
        }
    }
}
"#,
        registry: bytes_lifetime_registry,
        expect_flagged: false,
    },
    ShapeCase {
        id: "kani_turbofish_const_generic",
        verifier: Verifier::Kani,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    kani, VERIFY_ARRAY_INTO_ITER_SRC, {
        #[kani::proof]
        fn verify_array_into_iter_next_matches_original_element() {
            let a: i32 = kani::any();
            let b: i32 = kani::any();
            let c: i32 = kani::any();
            let mut it = [a, b, c].into_iter();
            assert!(RustStdStandard::<std::array::IntoIter<i32, 3>>::ensures((it.next(), Some(a))));
        }
    }
}
"#,
        registry: into_iter_const_generic_registry,
        // KNOWN GAP, deliberately unfixed -- see "Why the const-generic
        // case is harder than the other two" in
        // docs/planning/contract-bounds-shape-matrix.md. The comma inside
        // `IntoIter<i32, 3>` isn't nested in any `Group` token, so
        // `check_macro_call`'s top-level-comma split truncates the real
        // call, and it's flagged even though `RustStdStandard<..>` is
        // correctly registered and named above. Real production instance:
        // `crates/amenable_kani/src/rust_std/array.rs`.
        expect_flagged: true,
    },
    ShapeCase {
        id: "kani_turbofish_comma_bearing_generic",
        verifier: Verifier::Kani,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    kani, VERIFY_PAIR_LIKE_SRC, {
        #[kani::proof]
        fn verify_pair_like_ensures_matches() {
            let a: i32 = kani::any();
            let b: i32 = kani::any();
            assert!(RustStdStandard::<PairLike<i32, i32>>::ensures((a, b)));
        }
    }
}
"#,
        registry: pair_like_comma_generic_registry,
        // KNOWN GAP, same root cause as `kani_turbofish_const_generic`
        // (a non-const two-parameter generic instead of a const one) --
        // no real production instance yet, kept as a synthetic row so the
        // shape is documented and reproducible before one shows up.
        expect_flagged: true,
    },
];

#[test]
fn shape_matrix_matches_expected_flags() -> miette::Result<()> {
    cordial::init_tracing();
    let src_root = fixtures_root();
    for case in SHAPE_CASES {
        let path = src_root.join(format!("shape_{}.rs", case.id));
        let registry = (case.registry)();
        let scan = match case.verifier {
            Verifier::Kani => scan_kani_contract_bounds_source,
            Verifier::Creusot => scan_creusot_contract_bounds_source,
            Verifier::Verus => scan_verus_contract_bounds_source,
        };
        let findings = scan(case.source, &path, &src_root, &registry)
            .into_diagnostic()
            .wrap_err_with(|| format!("scan shape case {} ({})", case.id, case.kind))?;
        let flagged = !findings.is_empty();
        assert_eq!(
            flagged, case.expect_flagged,
            "shape case {:?} ({:?}, {}): expected flagged={}, got flagged={} (findings: {findings:?})",
            case.id, case.verifier, case.kind, case.expect_flagged, flagged,
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Coverage beyond the shape matrix: fixture-driven, multi-clause, and
// crate/session-level tests whose assertions inspect specific finding
// content or use a different entry point (`scan_crate_contract_bounds`,
// `SessionBuilder`) rather than a single source string's flagged/silent
// outcome. These stay as dedicated tests rather than table rows.
// ---------------------------------------------------------------------

#[test]
fn creusot_named_call_matching_a_registered_fn_name_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_creusot.rs";
    let src_root = fixtures_root();
    let registry = vec![logic_fn_record("creusot", "ensures", "char_roundtrips")];
    let findings = scan_creusot_contract_bounds_source(
        &fixture(name)?,
        &src_root.join(name),
        &src_root,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan creusot fixture")?;

    assert_eq!(findings.len(), 2);
    assert!(
        findings
            .iter()
            .any(|finding| finding.context.contains("verify_something_raw")
                && finding.snippet.contains("result >= 0"))
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.context.contains("verify_pearlite_only")
                && finding.snippet.contains("0xD7FF"))
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.context.contains("verify_char_roundtrip"))
    );
    Ok(())
}

#[test]
fn creusot_named_call_to_an_unregistered_fn_name_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_creusot.rs";
    let src_root = fixtures_root();
    let findings =
        scan_creusot_contract_bounds_source(&fixture(name)?, &src_root.join(name), &src_root, &[])
            .into_diagnostic()
            .wrap_err("scan creusot fixture")?;

    assert_eq!(findings.len(), 3);
    assert!(findings.iter().any(|finding| {
        finding.context.contains("verify_char_roundtrip")
            && finding.snippet.contains("char_roundtrips")
    }));
    Ok(())
}

#[test]
fn creusot_trivial_requires_true_is_never_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_creusot.rs";
    let src_root = fixtures_root();
    let findings =
        scan_creusot_contract_bounds_source(&fixture(name)?, &src_root.join(name), &src_root, &[])
            .into_diagnostic()
            .wrap_err("scan creusot fixture")?;

    assert!(!findings.iter().any(|finding| finding.snippet == "true"));
    Ok(())
}

#[test]
fn verus_named_call_matching_a_registered_fn_name_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_verus.rs";
    let src_root = fixtures_root();
    let registry = vec![logic_fn_record("verus", "ensures", "char_roundtrips")];
    let findings = scan_verus_contract_bounds_source(
        &fixture(name)?,
        &src_root.join(name),
        &src_root,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan verus fixture")?;

    assert_eq!(findings.len(), 1);
    assert!(findings[0].context.contains("verify_something_raw"));
    assert!(findings[0].snippet.contains("value >= 0"));
    Ok(())
}

#[test]
fn verus_assume_specification_semicolon_terminator_does_not_leak_into_the_next_item()
-> miette::Result<()> {
    cordial::init_tracing();
    // `assume_specification` (and any other body-less Verus declaration)
    // ends its `ensures`/`requires` list with a bare `;`, not a brace
    // group. Without a `;` stop case in the clause-list scanner, the scan
    // runs past the semicolon and swallows the next item's doc-comment
    // attributes and signature as if they were more of the same clause
    // list, manufacturing a phantom finding out of unrelated tokens.
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub assume_specification<'a, B: ToOwned + ?Sized> [Cow::<'a, B>::into_owned] (cow: Cow<'a, B>) -> (result: <B as ToOwned>::Owned)
    ensures
        cow_into_owned_preserves_variant_value(cow, result),
;

/// Unrelated doc comment on the next item -- must never be folded into
/// the `assume_specification` above's clause list.
pub fn verify_something(value: i32) -> (result: i32)
    ensures
        result == value,
{
    value
}

} // verus!
"#;
    let path = fixtures_root().join("inline_verus_assume_spec.rs");
    let registry = vec![logic_fn_record(
        "verus",
        "ensures",
        "cow_into_owned_preserves_variant_value",
    )];
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan inline verus assume_specification")?;

    // `cow_into_owned_preserves_variant_value(cow, result)` is a real
    // registered call, silenced. The only real finding left is
    // `verify_something`'s own `result == value` -- never a phantom
    // "clause" made of the semicolon, doc comment, and next signature.
    assert_eq!(findings.len(), 1);
    assert!(findings[0].context.contains("verify_something"));
    assert!(findings[0].snippet.contains("result == value"));
    Ok(())
}

#[test]
fn kani_named_call_matching_a_registered_type_is_not_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_kani.rs";
    let src_root = fixtures_root();
    let registry = vec![
        kani_type_record("requires", "fixture::NonNegative"),
        kani_type_record("ensures", "fixture::NonNegative"),
    ];
    let findings = scan_kani_contract_bounds_source(
        &fixture(name)?,
        &src_root.join(name),
        &src_root,
        &registry,
    )
    .into_diagnostic()
    .wrap_err("scan kani fixture")?;

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].rule_id,
        AntipatternRuleId::UnnamedContractBound001
    );
    assert!(findings[0].context.contains("verify_raw"));
    assert!(findings[0].snippet.contains("value < 100"));
    Ok(())
}

#[test]
fn kani_named_call_to_an_unregistered_type_is_flagged() -> miette::Result<()> {
    cordial::init_tracing();
    let name = "contract_bounds_kani.rs";
    let src_root = fixtures_root();
    let findings =
        scan_kani_contract_bounds_source(&fixture(name)?, &src_root.join(name), &src_root, &[])
            .into_diagnostic()
            .wrap_err("scan kani fixture")?;

    assert_eq!(findings.len(), 3);
    assert!(
        findings
            .iter()
            .any(|finding| finding.snippet.contains("NonNegative :: requires"))
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.snippet.contains("NonNegative :: ensures"))
    );
    Ok(())
}

#[test]
fn verus_bare_result_is_none_is_never_flagged_but_is_some_content_is() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_something() -> (result: (Option<i32>, Option<i32>, Option<i32>))
    ensures
        result.0 == Some(1),
        result.1 == Some(2),
        result.2 is None,
{
    (Some(1), Some(2), None)
}

} // verus!
"#;
    let path = fixtures_root().join("inline_verus_is_none.rs");
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan verus is_none")?;

    assert_eq!(findings.len(), 2);
    assert!(findings.iter().any(|finding| {
        finding.snippet.contains("Some")
            && finding.snippet.contains('1')
            && finding.snippet.contains("result . 0")
    }));
    assert!(findings.iter().any(|finding| {
        finding.snippet.contains("Some")
            && finding.snippet.contains('2')
            && finding.snippet.contains("result . 1")
    }));
    assert!(
        !findings
            .iter()
            .any(|finding| finding.snippet.contains("is None"))
    );
    Ok(())
}

#[test]
fn verus_bare_result_tuple_projections_are_never_flagged_but_comparisons_are() -> miette::Result<()>
{
    cordial::init_tracing();
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub fn verify_something(initial: i32, updated: i32) -> (result: (bool, bool, i32))
    ensures
        result.0,
        !result.1,
        result.2 == updated,
{
    (true, false, updated)
}

} // verus!
"#;
    let path = fixtures_root().join("inline_verus_tuple.rs");
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan verus tuple")?;

    assert_eq!(findings.len(), 1);
    assert!(findings[0].snippet.contains("result . 2 == updated"));
    Ok(())
}

#[test]
fn kani_raw_assert_eq_is_flagged_as_the_synthesized_equality() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
amenable_derive::harness! {
    kani, VERIFY_RAW_EQ_SRC, {
        #[kani::proof]
        fn verify_raw_eq() {
            let value: i32 = kani::any();
            assert_eq!(value, value + 0, "raw equation via assert_eq");
        }
    }
}
"#;
    let path = fixtures_root().join("inline_kani_assert_eq.rs");
    let findings = scan_kani_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan raw assert_eq kani")?;

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].rule_id,
        AntipatternRuleId::UnnamedContractBound001
    );
    assert!(findings[0].snippet.contains("value == value + 0"));
    Ok(())
}

#[test]
fn kani_raw_assume_is_now_flagged_when_unregistered() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
amenable_derive::harness! {
    kani, VERIFY_RAW_ASSUME_SRC, {
        #[kani::proof]
        fn verify_raw_assume() {
            let value: i32 = kani::any();
            kani::assume(value != 42);
            assert!(true);
        }
    }
}
"#;
    let path = fixtures_root().join("inline_kani_assume.rs");
    let findings = scan_kani_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan raw assume kani")?;

    assert_eq!(findings.len(), 1);
    assert!(findings[0].snippet.contains("value != 42"));
    Ok(())
}

#[test]
fn kani_gallery_directory_is_pruned_from_the_crate_walk() -> miette::Result<()> {
    cordial::init_tracing();
    let crate_root = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let src_root = crate_root.path().join("src");
    let gallery_root = src_root.join("gallery");
    fs::create_dir_all(&gallery_root)
        .into_diagnostic()
        .wrap_err("gallery dir")?;

    fs::write(
        src_root.join("lib.rs"),
        r#"
amenable_derive::harness! {
    kani, VERIFY_PRODUCTION_SRC, {
        #[kani::proof]
        fn verify_production() {
            let value: i32 = kani::any();
            assert!(value < 100, "raw bound");
        }
    }
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write production harness")?;

    fs::write(
        gallery_root.join("experiment.rs"),
        r#"
amenable_derive::harness! {
    kani, VERIFY_EXPERIMENT_SRC, {
        #[kani::proof]
        fn verify_gallery_experiment_times_out() {
            let value: i32 = kani::any();
            assert!(value < 100, "raw bound, same shape as the production one");
        }
    }
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write gallery harness")?;

    let findings = scan_crate_contract_bounds(crate_root.path(), "amenable_kani", &[])
        .into_diagnostic()
        .wrap_err("scan crate")?;

    assert_eq!(findings.len(), 1);
    assert!(findings[0].context.contains("verify_production"));
    assert!(!findings[0].file.starts_with("src/gallery"));
    Ok(())
}

#[test]
fn checklist_groups_same_shape_findings_into_a_duplicate_cluster() -> miette::Result<()> {
    cordial::init_tracing();
    let workspace = tempfile::tempdir().into_diagnostic().wrap_err("tempdir")?;
    let crate_root = workspace.path().join("amenable_kani");
    let src_root = crate_root.join("src");
    fs::create_dir_all(&src_root)
        .into_diagnostic()
        .wrap_err("src dir")?;

    fs::write(
        workspace.path().join("Cargo.toml"),
        r#"[workspace]
members = ["amenable_kani"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"
"#,
    )
    .into_diagnostic()
    .wrap_err("write workspace manifest")?;

    fs::write(
        crate_root.join("Cargo.toml"),
        r#"[package]
name = "amenable_kani"
version.workspace = true
edition.workspace = true
"#,
    )
    .into_diagnostic()
    .wrap_err("write crate manifest")?;

    fs::write(
        src_root.join("lib.rs"),
        "mod alloc_collections;\nmod alloc_ffi;\n",
    )
    .into_diagnostic()
    .wrap_err("write lib.rs")?;

    fs::write(
        src_root.join("alloc_collections.rs"),
        r#"
amenable_derive::harness! {
    kani, VERIFY_MAP_SRC, {
        #[kani::proof]
        fn verify_map() {
            let map: Vec<i32> = vec![];
            assert!(map.is_empty(), "message");
        }
    }
}

amenable_derive::harness! {
    kani, VERIFY_DEQUE_SRC, {
        #[kani::proof]
        fn verify_deque() {
            let dq: Vec<i32> = vec![];
            assert!(dq.is_empty(), "message");
        }
    }
}

amenable_derive::harness! {
    kani, VERIFY_HEAP_SRC, {
        #[kani::proof]
        fn verify_heap() {
            let heap: Vec<i32> = vec![];
            assert!(heap.is_empty(), "message");
        }
    }
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write cluster sources")?;

    fs::write(
        src_root.join("alloc_ffi.rs"),
        r#"
amenable_derive::harness! {
    kani, VERIFY_CSTRING_SRC, {
        #[kani::proof]
        fn verify_cstring() {
            let value: i32 = kani::any();
            assert!(value != 0, "different shape");
        }
    }
}
"#,
    )
    .into_diagnostic()
    .wrap_err("write lone source")?;

    let store = tempfile::tempdir().into_diagnostic().wrap_err("store")?;
    let session = SessionBuilder::new(workspace.path())
        .with_store_root(store.path())
        .register(&ANTIPATTERNS_ETIQUETTE)
        .build();
    session
        .run(&RunAll)
        .into_diagnostic()
        .wrap_err("antipatterns session")?;

    let checklist = fs::read_to_string(store.path().join("findings/antipatterns.checklist.md"))
        .into_diagnostic()
        .wrap_err("read checklist")?;

    assert!(checklist.contains("Possible duplicate clusters"));
    assert!(checklist.contains("X . is_empty ()"));
    assert!(checklist.contains("3 sites"));
    assert!(checklist.contains("verify_map"));
    assert!(checklist.contains("verify_deque"));
    assert!(checklist.contains("verify_heap"));
    let cluster_section = checklist
        .split("Possible duplicate clusters")
        .nth(1)
        .ok_or_else(|| miette::miette!("cluster section present"))?;
    let cluster_block = cluster_section.split("\n\n").next().unwrap_or_default();
    assert!(!cluster_block.contains("verify_cstring"));
    Ok(())
}

#[test]
fn verus_dotted_ensures_method_call_is_not_mistaken_for_the_clause_keyword() -> miette::Result<()> {
    cordial::init_tracing();
    let source = r#"
use verus_builtin_macros::verus;
use vstd::prelude::*;

verus! {

pub assume_specification<H: core::default::Default + Hasher> [<BuildHasherDefault<H> as BuildHasher>::build_hasher] (builder: &BuildHasherDefault<H>) -> (result: H)
    ensures
        H::default.ensures((), result),
;

} // verus!
"#;
    let path = fixtures_root().join("inline_verus_dotted_ensures_method.rs");
    let findings = scan_verus_contract_bounds_source(
        source,
        &path,
        path.parent().ok_or_else(|| miette::miette!("parent"))?,
        &[],
    )
    .into_diagnostic()
    .wrap_err("scan verus dotted-ensures method call")?;

    // The whole clause is genuinely unnamed (`H::default.ensures(..)` is
    // Verus's own builtin function-item contract inspection, not a call
    // to a registered predicate), so it's expected to still be flagged --
    // but as exactly ONE finding covering the whole clause, not split in
    // two at the method call's own literal `ensures` identifier.
    assert_eq!(findings.len(), 1, "findings: {findings:?}");
    assert_eq!(findings[0].snippet, "H :: default . ensures (() , result)");
    Ok(())
}
