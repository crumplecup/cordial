use cordial::{
    Etiquette, all_plugins, etiquettes_from_plugins, lookup_etiquette, quality_etiquettes,
    render_explain_list, render_explain_page,
};

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
        lookup_etiquette(&etiquettes, "doc_warnings")
            .ok_or_else(|| miette::miette!("doc_warnings"))?,
    );
    assert!(page.contains("# rustdoc warnings (`doc_warnings`)"));
    assert!(page.contains("## Why"));
    assert!(page.contains("## Logic"));
    assert!(page.contains("## Opt out"));
    assert!(page.contains("`DOC-WARNING-001`"));
    assert!(page.contains("[doc_warnings] enabled = false"));
    assert!(page.contains("cordial.toml"));
    Ok(())
}

#[test]
fn compiled_opt_out_points_at_cordial_toml() {
    cordial::init_tracing();
    for etiquette in etiquettes_from_plugins(&all_plugins()) {
        let opt_out = etiquette.explain().opt_out();
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
    let etiquettes = etiquettes_from_plugins(&all_plugins());
    let mut seen = std::collections::BTreeSet::new();
    for etiquette in &etiquettes {
        assert_explain_filled(*etiquette);
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
