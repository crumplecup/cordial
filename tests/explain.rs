use std::sync::Arc;

use cordial::{
    Etiquette, SessionBuilder, all_plugins, etiquettes_from_plugins, lookup_etiquette,
    quality_etiquettes, render_explain_list, render_explain_page,
};

/// Every etiquette the compiled plugins contribute under default config.
fn compiled_etiquettes() -> Vec<Arc<dyn Etiquette>> {
    etiquettes_from_plugins(&all_plugins(), &SessionBuilder::new(".").build())
}

fn assert_explain_filled(etiquette: &dyn Etiquette) {
    let explain = etiquette.explain();
    assert!(!explain.summary().is_empty(), "{} summary", etiquette.id());
    assert!(!explain.why().is_empty(), "{} why", etiquette.id());
    assert!(!explain.logic().is_empty(), "{} logic", etiquette.id());
    assert!(!explain.opt_out().is_empty(), "{} opt_out", etiquette.id());
}

#[test]
fn every_quality_etiquette_fills_explain() {
    cordial::init_tracing();
    let etiquettes = quality_etiquettes();
    assert!(!etiquettes.is_empty());
    for etiquette in &etiquettes {
        assert_explain_filled(*etiquette);
    }
}

#[test]
fn lookup_accepts_etiquette_id_and_rule_id() -> miette::Result<()> {
    cordial::init_tracing();
    let etiquettes = quality_etiquettes();
    let by_id = lookup_etiquette(&etiquettes, "doc_warnings")
        .ok_or_else(|| miette::miette!("doc_warnings"))?;
    let by_rule = lookup_etiquette(&etiquettes, "DOC-WARNING-001")
        .ok_or_else(|| miette::miette!("DOC-WARNING-001"))?;
    assert_eq!(by_id.id(), by_rule.id());
    assert_eq!(by_id.id(), "doc_warnings");
    assert!(lookup_etiquette(&etiquettes, "not-a-real-lint").is_none());
    Ok(())
}

#[test]
fn render_list_and_page() -> miette::Result<()> {
    cordial::init_tracing();
    let etiquettes = quality_etiquettes();
    let list = render_explain_list(&etiquettes);
    assert!(list.contains("doc_warnings"));
    assert!(list.contains("Does cargo doc emit rustdoc::* diagnostics rustc never sees?"));

    let page = render_explain_page(
        lookup_etiquette(&etiquettes, "visibility").ok_or_else(|| miette::miette!("visibility"))?,
    );
    assert!(page.contains("# Module visibility (`visibility`)"));
    assert!(page.contains("## Why"));
    assert!(page.contains("## Logic"));
    assert!(page.contains("## Opt out"));
    assert!(page.contains("`VIS-CRATE-FLAT-001`"));
    assert!(page.contains("[visibility] enabled = false"));
    assert!(page.contains("cordial.toml"));

    let owned = compiled_etiquettes();
    let coverage: Vec<&dyn Etiquette> = owned.iter().map(|etiquette| etiquette.as_ref()).collect();
    let impl_coverage = render_explain_page(
        lookup_etiquette(&coverage, "impl-coverage")
            .ok_or_else(|| miette::miette!("impl-coverage"))?,
    );
    assert!(!impl_coverage.contains("## Resolution order"));

    let tracing = render_explain_page(
        lookup_etiquette(&etiquettes, "TRACING-MISSING-INSTRUMENT")
            .ok_or_else(|| miette::miette!("tracing"))?,
    );
    assert!(tracing.contains("## Resolution order"));
    assert!(tracing.contains("Tracing after derive patterns"));
    assert!(tracing.contains("`DERIVE-NEW-001`"));
    assert!(tracing.contains("`INLINE-TEST-MOD`"));

    let derives = render_explain_page(
        lookup_etiquette(&etiquettes, "DERIVE-NEW-001")
            .ok_or_else(|| miette::miette!("derives"))?,
    );
    assert!(derives.contains("## Resolution order"));
    assert!(derives.contains("Derive patterns after crate attributes"));
    assert!(derives.contains("`CRATE-FORBID-UNSAFE-001`"));
    assert!(derives.contains("`TRACING-MISSING-INSTRUMENT`"));

    let foreign = render_explain_page(
        lookup_etiquette(&etiquettes, "FOREIGN-ERROR-CANDIDATE")
            .ok_or_else(|| miette::miette!("foreign_error_types"))?,
    );
    assert!(foreign.contains("## Resolution order"));
    assert!(foreign.contains("Foreign error types after error handling"));
    assert!(foreign.contains("`PANIC-SOURCE-PANIC`"));
    assert!(foreign.contains("`ANTIPATTERN-BOX-DYN-ERROR-001`"));

    let antipatterns = render_explain_page(
        lookup_etiquette(&etiquettes, "ANTIPATTERN-BOX-DYN-ERROR-001")
            .ok_or_else(|| miette::miette!("antipatterns"))?,
    );
    assert!(antipatterns.contains("## Resolution order"));
    assert!(antipatterns.contains("Antipatterns after foreign error types"));
    assert!(antipatterns.contains("`FOREIGN-ERROR-CANDIDATE`"));
    assert!(antipatterns.contains("`CLI-ISLAND-001`"));

    let cli_layout = render_explain_page(
        lookup_etiquette(&etiquettes, "CLI-ISLAND-001")
            .ok_or_else(|| miette::miette!("cli_layout"))?,
    );
    assert!(cli_layout.contains("## Resolution order"));
    assert!(cli_layout.contains("CLI layout after antipatterns"));
    assert!(cli_layout.contains("`ANTIPATTERN-BOX-DYN-ERROR-001`"));
    assert!(cli_layout.contains("`ALLOW-ATTR-001`"));

    let allows = render_explain_page(
        lookup_etiquette(&etiquettes, "ALLOW-ATTR-001").ok_or_else(|| miette::miette!("allows"))?,
    );
    assert!(allows.contains("## Resolution order"));
    assert!(allows.contains("Allow attributes after CLI layout"));
    assert!(allows.contains("`CLI-ISLAND-001`"));
    assert!(allows.contains("`CRATE-FORBID-UNSAFE-001`"));

    let crate_attrs = render_explain_page(
        lookup_etiquette(&etiquettes, "CRATE-FORBID-UNSAFE-001")
            .ok_or_else(|| miette::miette!("crate_attrs"))?,
    );
    assert!(crate_attrs.contains("## Resolution order"));
    assert!(crate_attrs.contains("Crate attributes after allow attributes"));
    assert!(crate_attrs.contains("`ALLOW-ATTR-001`"));
    assert!(crate_attrs.contains("`DERIVE-NEW-001`"));

    let inline_tests = render_explain_page(
        lookup_etiquette(&etiquettes, "INLINE-TEST-MOD")
            .ok_or_else(|| miette::miette!("inline_tests"))?,
    );
    assert!(inline_tests.contains("## Resolution order"));
    assert!(inline_tests.contains("Inline tests after tracing"));
    assert!(inline_tests.contains("`TRACING-MISSING-INSTRUMENT`"));
    assert!(inline_tests.contains("`MODULARITY-FILE`"));

    let modularity = render_explain_page(
        lookup_etiquette(&etiquettes, "MODULARITY-FILE")
            .ok_or_else(|| miette::miette!("modularity"))?,
    );
    assert!(modularity.contains("## Resolution order"));
    assert!(modularity.contains("Modularity after inline tests"));
    assert!(modularity.contains("`INLINE-TEST-MOD`"));
    assert!(modularity.contains("`CFG-SCATTER-001`"));

    let cfg_scatter = render_explain_page(
        lookup_etiquette(&etiquettes, "CFG-SCATTER-001")
            .ok_or_else(|| miette::miette!("cfg_scatter"))?,
    );
    assert!(cfg_scatter.contains("## Resolution order"));
    assert!(cfg_scatter.contains("Cfg scatter after modularity"));
    assert!(cfg_scatter.contains("`MODULARITY-FILE`"));
    assert!(cfg_scatter.contains("`UNEXPECTED-CFG-001`"));

    let cfg_hygiene = render_explain_page(
        lookup_etiquette(&etiquettes, "UNEXPECTED-CFG-001")
            .ok_or_else(|| miette::miette!("cfg_hygiene"))?,
    );
    assert!(cfg_hygiene.contains("## Resolution order"));
    assert!(cfg_hygiene.contains("Cfg hygiene after cfg scatter"));
    assert!(cfg_hygiene.contains("`CFG-SCATTER-001`"));
    assert!(cfg_hygiene.contains("`VIS-CRATE-FLAT-001`"));

    let visibility = render_explain_page(
        lookup_etiquette(&etiquettes, "VIS-CRATE-FLAT-001")
            .ok_or_else(|| miette::miette!("visibility"))?,
    );
    assert!(visibility.contains("## Resolution order"));
    assert!(visibility.contains("Module visibility after cfg hygiene"));
    assert!(visibility.contains("`UNEXPECTED-CFG-001`"));
    assert!(visibility.contains("`GLOB-IMPORT-001`"));

    let glob_imports = render_explain_page(
        lookup_etiquette(&etiquettes, "GLOB-IMPORT-001")
            .ok_or_else(|| miette::miette!("glob_imports"))?,
    );
    assert!(glob_imports.contains("## Resolution order"));
    assert!(glob_imports.contains("Glob imports after module visibility"));
    assert!(glob_imports.contains("`VIS-CRATE-FLAT-001`"));
    assert!(glob_imports.contains("`PAGEANTRY-TRAIT-001`"));

    let pageantry = render_explain_page(
        lookup_etiquette(&etiquettes, "PAGEANTRY-TRAIT-001")
            .ok_or_else(|| miette::miette!("pageantry"))?,
    );
    assert!(pageantry.contains("## Resolution order"));
    assert!(pageantry.contains("Pageantry after glob imports"));
    assert!(pageantry.contains("`GLOB-IMPORT-001`"));
    assert!(pageantry.contains("`DEPENDENCY-FRESHNESS-PATCH`"));

    let freshness = render_explain_page(
        lookup_etiquette(&etiquettes, "DEPENDENCY-FRESHNESS-PATCH")
            .ok_or_else(|| miette::miette!("dependency_freshness"))?,
    );
    assert!(freshness.contains("## Resolution order"));
    assert!(freshness.contains("Dependency freshness after pageantry"));
    assert!(freshness.contains("`PAGEANTRY-TRAIT-001`"));
    assert!(freshness.contains("`DOC-WARNING-001`"));

    let rustdoc = render_explain_page(
        lookup_etiquette(&etiquettes, "DOC-WARNING-001")
            .ok_or_else(|| miette::miette!("doc_warnings"))?,
    );
    assert!(rustdoc.contains("## Resolution order"));
    assert!(rustdoc.contains("Rustdoc warnings after dependency freshness"));
    assert!(rustdoc.contains("`DEPENDENCY-FRESHNESS-PATCH`"));
    assert!(rustdoc.contains("`CREUSOT-DIAGNOSTIC-001`"));

    let creusot = render_explain_page(
        lookup_etiquette(&etiquettes, "CREUSOT-DIAGNOSTIC-001")
            .ok_or_else(|| miette::miette!("creusot_diagnostics"))?,
    );
    assert!(creusot.contains("## Resolution order"));
    assert!(creusot.contains("Creusot diagnostics after rustdoc warnings"));
    assert!(creusot.contains("`DOC-WARNING-001`"));
    assert!(creusot.contains("`VERUS-WARNING-001`"));

    let verus = render_explain_page(
        lookup_etiquette(&etiquettes, "VERUS-WARNING-001")
            .ok_or_else(|| miette::miette!("verus_warnings"))?,
    );
    assert!(verus.contains("## Resolution order"));
    assert!(verus.contains("Verus warnings after Creusot diagnostics"));
    assert!(verus.contains("`CREUSOT-DIAGNOSTIC-001`"));
    assert!(verus.contains("`PROOF-PATTERN-ASSUME`"));

    let proof = render_explain_page(
        lookup_etiquette(&etiquettes, "PROOF-PATTERN-ASSUME")
            .ok_or_else(|| miette::miette!("proof_patterns"))?,
    );
    assert!(proof.contains("## Resolution order"));
    assert!(proof.contains("Proof patterns after Verus warnings"));
    assert!(proof.contains("`VERUS-WARNING-001`"));
    Ok(())
}

#[test]
fn compiled_opt_out_points_at_cordial_toml() {
    cordial::init_tracing();
    for etiquette in compiled_etiquettes() {
        let explain = etiquette.explain();
        let opt_out = explain.opt_out();
        assert!(
            opt_out.contains("cordial.toml"),
            "{} opt_out should name cordial.toml, got {opt_out}",
            etiquette.id()
        );
        assert!(
            opt_out.contains("enabled = false"),
            "{} opt_out should name the enabled = false line, got {opt_out}",
            etiquette.id()
        );
        assert!(
            !opt_out.contains("exceptions"),
            "{} opt_out should not point at exceptions, got {opt_out}",
            etiquette.id()
        );
    }
}

#[test]
fn compiled_plugins_have_unique_explain_ids() {
    cordial::init_tracing();
    let etiquettes = compiled_etiquettes();
    let mut seen = std::collections::BTreeSet::new();
    for etiquette in &etiquettes {
        assert_explain_filled(etiquette.as_ref());
        assert!(
            seen.insert(etiquette.id()),
            "duplicate etiquette id {}",
            etiquette.id()
        );
    }
}

#[cfg(feature = "elicitation")]
#[test]
fn coverage_etiquettes_fill_explain() {
    cordial::init_tracing();
    for etiquette in cordial::coverage_etiquettes() {
        assert_explain_filled(etiquette);
    }
}
