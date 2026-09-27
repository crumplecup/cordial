use cordial::{CordialErrorKind, LintConstraint, LintOrder, OrderExplain};

const EXPLAIN: OrderExplain = OrderExplain::new("test", "test rationale");

#[test]
fn after_a_before_b_then_c_after_b_is_a_b_c() -> miette::Result<()> {
    cordial::init_tracing();
    const KNOWN: &[&str] = &["A", "B", "C"];
    const CONSTRAINTS: &[LintConstraint] = &[
        LintConstraint::new("B", &["A"], EXPLAIN),
        LintConstraint::new("C", &["B"], EXPLAIN),
    ];
    let order =
        LintOrder::try_from(KNOWN, CONSTRAINTS).map_err(|error| miette::miette!("{error}"))?;
    assert_eq!(order.ranked_ids(), vec!["A", "B", "C"]);
    Ok(())
}

#[test]
fn d_after_c_and_before_a_is_a_cycle() -> miette::Result<()> {
    cordial::init_tracing();
    const KNOWN: &[&str] = &["A", "B", "C", "D"];
    const CYCLE: &[LintConstraint] = &[
        LintConstraint::new("B", &["A"], EXPLAIN),
        LintConstraint::new("C", &["B"], EXPLAIN),
        LintConstraint::new("D", &["C"], EXPLAIN),
        LintConstraint::new("A", &["D"], EXPLAIN),
    ];
    let err = LintOrder::try_from(KNOWN, CYCLE)
        .err()
        .ok_or_else(|| miette::miette!("expected cycle"))?;
    assert!(matches!(err.kind(), CordialErrorKind::LintOrderCycle(_)));
    Ok(())
}

#[test]
fn dangling_after_target_is_an_error() -> miette::Result<()> {
    cordial::init_tracing();
    const KNOWN: &[&str] = &["A"];
    const CONSTRAINTS: &[LintConstraint] = &[LintConstraint::new("A", &["missing"], EXPLAIN)];
    let err = LintOrder::try_from(KNOWN, CONSTRAINTS)
        .err()
        .ok_or_else(|| miette::miette!("expected dangling after target"))?;
    assert!(matches!(err.kind(), CordialErrorKind::LintOrderDangling(_)));
    Ok(())
}

#[test]
fn built_in_order_places_derives_before_tracing() -> miette::Result<()> {
    cordial::init_tracing();
    let ranked = cordial::BUILT_IN_ORDER.ranked_ids();
    let derive = ranked
        .iter()
        .position(|id| *id == "DERIVE-NEW-001")
        .ok_or_else(|| miette::miette!("derive"))?;
    let tracing = ranked
        .iter()
        .position(|id| *id == "TRACING-MISSING-INSTRUMENT")
        .ok_or_else(|| miette::miette!("tracing"))?;
    assert!(derive < tracing);
    assert_eq!(
        cordial::BUILT_IN_ORDER
            .predecessors("TRACING-MISSING-INSTRUMENT")
            .first()
            .copied(),
        Some("DERIVE-BUILDER-001")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .successors("DERIVE-NEW-001")
            .contains(&"TRACING-MISSING-INSTRUMENT")
    );
    Ok(())
}

#[test]
fn built_in_order_places_error_handling_before_foreign_error_types() -> miette::Result<()> {
    cordial::init_tracing();
    let ranked = cordial::BUILT_IN_ORDER.ranked_ids();
    let panic = ranked
        .iter()
        .position(|id| *id == "PANIC-SOURCE-PANIC")
        .ok_or_else(|| miette::miette!("panic"))?;
    let foreign = ranked
        .iter()
        .position(|id| *id == "FOREIGN-ERROR-CANDIDATE")
        .ok_or_else(|| miette::miette!("foreign error types"))?;
    assert!(panic < foreign);
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("FOREIGN-ERROR-CANDIDATE")
            .contains(&"PANIC-SOURCE-PANIC")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .successors("PANIC-SOURCE-PANIC")
            .contains(&"FOREIGN-ERROR-CANDIDATE")
    );
    Ok(())
}

#[test]
fn built_in_order_places_foreign_error_types_before_antipatterns() -> miette::Result<()> {
    cordial::init_tracing();
    let ranked = cordial::BUILT_IN_ORDER.ranked_ids();
    let foreign = ranked
        .iter()
        .position(|id| *id == "FOREIGN-ERROR-CANDIDATE")
        .ok_or_else(|| miette::miette!("foreign error types"))?;
    let antipattern = ranked
        .iter()
        .position(|id| *id == "ANTIPATTERN-BOX-DYN-ERROR-001")
        .ok_or_else(|| miette::miette!("antipatterns"))?;
    assert!(foreign < antipattern);
    assert_eq!(
        cordial::BUILT_IN_ORDER
            .predecessors("ANTIPATTERN-BOX-DYN-ERROR-001")
            .first()
            .copied(),
        Some("FOREIGN-ERROR-CANDIDATE")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .successors("FOREIGN-ERROR-CANDIDATE")
            .contains(&"ANTIPATTERN-BOX-DYN-ERROR-001")
    );
    Ok(())
}

#[test]
fn built_in_order_places_antipatterns_before_cli_layout() -> miette::Result<()> {
    cordial::init_tracing();
    let ranked = cordial::BUILT_IN_ORDER.ranked_ids();
    let antipattern = ranked
        .iter()
        .position(|id| *id == "ANTIPATTERN-BOX-DYN-ERROR-001")
        .ok_or_else(|| miette::miette!("antipatterns"))?;
    let cli = ranked
        .iter()
        .position(|id| *id == "CLI-ISLAND-001")
        .ok_or_else(|| miette::miette!("cli layout"))?;
    assert!(antipattern < cli);
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("CLI-ISLAND-001")
            .contains(&"ANTIPATTERN-BOX-DYN-ERROR-001")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .successors("ANTIPATTERN-BOX-DYN-ERROR-001")
            .contains(&"CLI-ISLAND-001")
    );
    Ok(())
}

#[test]
fn built_in_order_places_cli_layout_before_allows() -> miette::Result<()> {
    cordial::init_tracing();
    let ranked = cordial::BUILT_IN_ORDER.ranked_ids();
    let cli = ranked
        .iter()
        .position(|id| *id == "CLI-ISLAND-001")
        .ok_or_else(|| miette::miette!("cli layout"))?;
    let allow = ranked
        .iter()
        .position(|id| *id == "ALLOW-ATTR-001")
        .ok_or_else(|| miette::miette!("allows"))?;
    assert!(cli < allow);
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("ALLOW-ATTR-001")
            .contains(&"CLI-ISLAND-001")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .successors("CLI-ISLAND-001")
            .contains(&"ALLOW-ATTR-001")
    );
    Ok(())
}

#[test]
fn built_in_order_places_style_chain_after_allows() -> miette::Result<()> {
    cordial::init_tracing();
    let ranked = cordial::BUILT_IN_ORDER.ranked_ids();
    let chain = [
        "ALLOW-ATTR-001",
        "CRATE-FORBID-UNSAFE-001",
        "DERIVE-NEW-001",
        "TRACING-MISSING-INSTRUMENT",
        "INLINE-TEST-MOD",
        "MODULARITY-FILE",
        "CFG-SCATTER-001",
        "UNEXPECTED-CFG-001",
        "VIS-CRATE-FLAT-001",
        "GLOB-IMPORT-001",
        "PAGEANTRY-TRAIT-001",
        "DEPENDENCY-FRESHNESS-PATCH",
        "DOC-WARNING-001",
        "CREUSOT-DIAGNOSTIC-001",
        "VERUS-WARNING-001",
        "PROOF-PATTERN-ASSUME",
    ];
    let mut last = 0;
    for id in chain {
        let position = ranked
            .iter()
            .position(|known| *known == id)
            .ok_or_else(|| miette::miette!("{id}"))?;
        assert!(position > last, "{id} should follow the previous link");
        last = position;
    }
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("CRATE-FORBID-UNSAFE-001")
            .contains(&"ALLOW-ATTR-001")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("DERIVE-NEW-001")
            .contains(&"CRATE-FORBID-UNSAFE-001")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("INLINE-TEST-MOD")
            .contains(&"TRACING-MISSING-INSTRUMENT")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("MODULARITY-FILE")
            .contains(&"INLINE-TEST-MOD")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("CFG-SCATTER-001")
            .contains(&"MODULARITY-FILE")
    );
    assert_eq!(
        cordial::BUILT_IN_ORDER
            .predecessors("UNEXPECTED-CFG-001")
            .first()
            .copied(),
        Some("CFG-SCATTER-001")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("VIS-CRATE-FLAT-001")
            .contains(&"UNEXPECTED-CFG-001")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("GLOB-IMPORT-001")
            .contains(&"VIS-CRATE-FLAT-001")
    );
    assert_eq!(
        cordial::BUILT_IN_ORDER
            .predecessors("PAGEANTRY-TRAIT-001")
            .first()
            .copied(),
        Some("GLOB-IMPORT-001")
    );
    assert_eq!(
        cordial::BUILT_IN_ORDER
            .predecessors("DEPENDENCY-FRESHNESS-PATCH")
            .first()
            .copied(),
        Some("PAGEANTRY-TRAIT-001")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("DOC-WARNING-001")
            .contains(&"DEPENDENCY-FRESHNESS-PATCH")
    );
    assert_eq!(
        cordial::BUILT_IN_ORDER
            .predecessors("CREUSOT-DIAGNOSTIC-001")
            .first()
            .copied(),
        Some("DOC-WARNING-001")
    );
    assert!(
        cordial::BUILT_IN_ORDER
            .predecessors("VERUS-WARNING-001")
            .contains(&"CREUSOT-DIAGNOSTIC-001")
    );
    assert_eq!(
        cordial::BUILT_IN_ORDER
            .predecessors("PROOF-PATTERN-ASSUME")
            .first()
            .copied(),
        Some("VERUS-WARNING-001")
    );
    Ok(())
}
