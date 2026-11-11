//! Shape matrix, Kani: call sites, negations, qualified paths, and turbofish type arguments.
#![cfg(feature = "antipatterns")]

use cordial::{ContractRecordDump, scan_kani_contract_bounds_source};

#[path = "support/contract_bounds_dump.rs"]
mod contract_bounds_dump;

#[path = "support/contract_bounds_kani_record.rs"]
mod contract_bounds_kani_record;

#[path = "support/contract_bounds_shape_case.rs"]
mod contract_bounds_shape_case;

use contract_bounds_dump::contract_record_dump;
use contract_bounds_kani_record::kani_type_record;
use contract_bounds_shape_case::{ShapeCase, check_cases};

fn abbreviated_ensures_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![contract_record_dump(
        "amenable_std::rust_std::RustStdStandard<i32>",
        "kani",
        "ensures",
        "value >= 0",
    )?])
}

fn nested_generic_cell_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![contract_record_dump(
        "amenable_std::rust_std::RustStdStandard<Cell<i32>>",
        "kani",
        "ensures",
        "actual == expected",
    )?])
}

fn nonnegative_ensures_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![kani_type_record("ensures", "fixture::NonNegative")?])
}

fn bytes_lifetime_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![kani_type_record(
        "ensures",
        "amenable_std::rust_std::RustStdStandard<std::str::Bytes<'static>>",
    )?])
}

fn into_iter_const_generic_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![kani_type_record(
        "ensures",
        "amenable_std::rust_std::RustStdStandard<std::array::IntoIter<i32, 3>>",
    )?])
}

fn pair_like_comma_generic_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![kani_type_record(
        "ensures",
        "amenable_std::rust_std::RustStdStandard<PairLike<i32, i32>>",
    )?])
}

fn extract_if_trailing_comma_registry() -> miette::Result<Vec<ContractRecordDump>> {
    Ok(vec![kani_type_record(
        "ensures",
        "amenable_std::rust_std::RustStdStandard<std::collections::linked_list::ExtractIf<'static, i32, fn(&mut i32) -> bool>>",
    )?])
}

const SHAPE_CASES: &[ShapeCase] = &[
    ShapeCase {
        id: "kani_type_suffix_abbreviated_call_site",
        scan: scan_kani_contract_bounds_source,
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
        scan: scan_kani_contract_bounds_source,
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
        scan: scan_kani_contract_bounds_source,
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
        scan: scan_kani_contract_bounds_source,
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
        scan: scan_kani_contract_bounds_source,
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
        scan: scan_kani_contract_bounds_source,
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
        id: "kani_turbofish_nested_generic_lifetime",
        scan: scan_kani_contract_bounds_source,
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
        scan: scan_kani_contract_bounds_source,
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
        // FIXED 2026-08-30 -- was the first documented gap in
        // docs/planning/contract-bounds-shape-matrix.md ("Why the
        // const-generic case is harder than the other two"). The comma
        // inside `IntoIter<i32, 3>` isn't nested in any `Group` token, so
        // the old naive top-level-comma split in `check_macro_call`
        // truncated the real call and flagged it even though
        // `RustStdStandard<..>` was correctly registered and named.
        // `check_macro_call` now parses `assert!`/`assert_eq!`'s argument
        // tokens as real `Punctuated<Expr, Comma>` grammar instead of a
        // hand-rolled split, which disambiguates a turbofish's internal
        // comma the same way rustc itself does -- eliminates this whole
        // bug class rather than special-casing const generics. Real
        // production instance: `crates/amenable_kani/src/rust_std/array.rs`.
        expect_flagged: false,
    },
    ShapeCase {
        id: "kani_turbofish_comma_bearing_generic",
        scan: scan_kani_contract_bounds_source,
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
        // FIXED 2026-08-30, same root cause and same fix as
        // `kani_turbofish_const_generic` (a non-const two-parameter
        // generic instead of a const one) -- no real production instance,
        // kept as a synthetic row so the shape stays covered.
        expect_flagged: false,
    },
    ShapeCase {
        id: "kani_turbofish_trailing_comma",
        scan: scan_kani_contract_bounds_source,
        kind: "ensures",
        source: r#"
amenable_derive::harness! {
    kani, VERIFY_EXTRACT_IF_SRC, {
        #[kani::proof]
        fn verify_extract_if_yields_matching_elements() {
            let value: i32 = kani::any();
            assert!(
                RustStdStandard::<
                    std::collections::linked_list::ExtractIf<'static, i32, fn(&mut i32) -> bool>,
                >::ensures((value, value)),
                "message"
            );
        }
    }
}
"#,
        registry: extract_if_trailing_comma_registry,
        // Found via real dogfooding (2026-08-30) immediately after fixing
        // `kani_turbofish_const_generic`/`kani_turbofish_comma_bearing_generic`:
        // re-running the scanner against `amenable` after those fixes
        // landed showed `array.rs`'s sites cleared, but a NEW site
        // (`alloc_collections.rs`'s real
        // `verify_linked_list_extract_if_partitions_by_the_predicate`)
        // was still flagged, a different bug in the same neighborhood --
        // a *trailing* comma inside the turbofish itself
        // (`RustStdStandard::<\n    ExtractIf<..>,\n>::ensures(..)`, a
        // call site written across multiple lines with a trailing comma
        // before the closing `>`, both valid, elidable Rust syntax).
        // `check_macro_call`'s real-`syn::Expr` parsing (the earlier fix)
        // captures the whole clause correctly; `matches_named_call`'s own
        // type-prefix suffix match was the remaining bug -- `syn`
        // preserves a source trailing comma through `ToTokens`, so the
        // re-emitted prefix ended `...bool>,>` against a hand-written
        // `evidence` string ending `...bool>>`, an otherwise-correct
        // suffix match failing on a comma with no semantic meaning. Fixed
        // via `canonicalize_type_text` collapsing `,>` to `>`.
        expect_flagged: false,
    },
];

#[test]
fn shape_matrix_matches_expected_flags() -> miette::Result<()> {
    cordial::init_tracing();
    check_cases(SHAPE_CASES)
}
