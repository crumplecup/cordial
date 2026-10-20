#![cfg(feature = "quality")]

use miette::IntoDiagnostic;

use cordial::{
    ChecklistSource, CordialConfig, Disposition, Finding, FindingSink, FindingSource, IrAnchor,
    ItemSource, NodeAnchor, NodeId, QualityAreaSummary, QualityChecklist, QualityReport, Rule,
    StoreLayout, Validity, Worklist, load_cordial_config, render_open, render_stats,
};

struct SampleRule(String);

impl Rule for SampleRule {
    fn id(&self) -> &str {
        &self.0
    }

    fn category(&self) -> &str {
        "panics"
    }

    fn description(&self) -> &str {
        "sample"
    }
}

struct SampleFinding {
    rule: SampleRule,
    anchor: NodeAnchor,
    line: u32,
}

impl Finding for SampleFinding {
    fn rule(&self) -> &dyn Rule {
        &self.rule
    }

    fn disposition(&self) -> Disposition {
        Disposition::Open
    }

    fn anchor(&self) -> &dyn IrAnchor {
        &self.anchor
    }

    fn emit(&self, sink: &mut dyn FindingSink) {
        sink.field("crate", &"sample");
        sink.field("file", &"src/store.rs");
        sink.field("line", &self.line);
        sink.field("context", &"read_all");
    }
}

#[test]
fn finding_source_adds_exact_rule_and_exception_command() -> miette::Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    let store = StoreLayout::from_root(root.path(), "fixture");
    std::fs::create_dir_all(store.findings_dir()).into_diagnostic()?;
    std::fs::write(
        store.findings_dir().join("panics.checklist.md"),
        "## `sample`\n\n- [ ] `read_all` — `src/store.rs:52` — `unwrap`\n",
    )
    .into_diagnostic()?;
    let finding = SampleFinding {
        rule: SampleRule("PANIC-SOURCE-UNWRAP".to_string()),
        anchor: NodeAnchor::new(NodeId::new(1)),
        line: 52,
    };
    let wrong_site = SampleFinding {
        rule: SampleRule("PANIC-SOURCE-PANIC".to_string()),
        anchor: NodeAnchor::new(NodeId::new(2)),
        line: 53,
    };
    let source =
        FindingSource::from_findings(ChecklistSource::new(&store), &[&finding, &wrong_site]);
    let report = QualityReport::new(
        vec![QualityAreaSummary::new(
            1,
            "Error handling".to_string(),
            1,
            "panics.checklist.md".to_string(),
            vec![QualityChecklist::new(
                "panics.checklist.md".to_string(),
                vec![],
            )],
            "summary.md".to_string(),
            String::new(),
        )],
        1,
    );
    let mut list =
        Worklist::build(&report, &store, &source, &CordialConfig::default()).into_diagnostic()?;
    assert_eq!(
        list.items()[0].rule_id().as_deref(),
        Some("PANIC-SOURCE-UNWRAP")
    );
    list.activate();
    let rendered = render_open(&list);
    assert!(rendered.contains("cordial explain PANIC-SOURCE-UNWRAP"));
    assert!(rendered.contains("cordial exceptions add panics --file 'src/store.rs' --line 52 --rule-id 'PANIC-SOURCE-UNWRAP' --crate-name 'sample'"));
    Ok(())
}

fn area(checklists: Vec<QualityChecklist>) -> QualityAreaSummary {
    QualityAreaSummary::new(
        1,
        "Error handling".to_string(),
        3,
        checklists[0].name().clone(),
        checklists,
        "summary.md".to_string(),
        String::new(),
    )
}

#[test]
fn checklist_source_keeps_guidance_details_and_filters_rules() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = tempfile::tempdir().into_diagnostic()?;
    std::fs::write(
        findings.path().join("panics.checklist.md"),
        "# Panic sources checklist\n\n## `crate`\n\n### Library\n\n_Fix library aborts._\n\n#### unwrap\n\n- [ ] `read_all` — `src/store.rs:52` — `unwrap`\n  - source: io::Error\n\n- [x] documented exception\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        findings.path().join("antipatterns.checklist.md"),
        "# Antipatterns\n\n## `crate`\n\n### ANTIPATTERN-BOX-DYN-ERROR-001\n\n- [ ] `boxed` — `src/lib.rs:10`\n\n### ANTIPATTERN-UNUSED-UNDERSCORE-ARG-001\n\n- [ ] `unused` — `src/lib.rs:20`\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        findings
            .path()
            .join("foreign-error-attenuation.checklist.md"),
        "## `crate`\n\n### Chain break (bad code → migrate)\n\n- [ ] `lost_source` — `src/error.rs:4`\n\n### Neutral (manual review)\n\n- [ ] `review_only` — `src/error.rs:8`\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        findings.path().join("cfg-scatter.checklist.md"),
        "# Scattered cfg\n\n## `crate`\n\n- [ ] `src/lib.rs` — `cfg(test)`\n  - sample: gated method\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        findings.path().join("modularity.checklist.md"),
        "# Modularity\n\n## `crate`\n\n### Hotspots\n\n- [ ] `src/large.rs` — **310 lines**\n  - [ ] `method_a` — split this body\n",
    )
    .into_diagnostic()?;
    std::fs::write(
        findings.path().join("visibility.checklist.md"),
        "# Visibility\n\n## `crate`\n\n- [ ] `VISIBILITY-PUB-001` — `src/lib.rs:5` — narrow this item\n",
    )
    .into_diagnostic()?;
    let source = ChecklistSource::from_findings_dir(findings.path());
    let area = area(vec![
        QualityChecklist::new("panics.checklist.md".to_string(), vec![]),
        QualityChecklist::new(
            "antipatterns.checklist.md".to_string(),
            vec!["ANTIPATTERN-BOX-DYN-ERROR-001".to_string()],
        ),
        QualityChecklist::new(
            "foreign-error-attenuation.checklist.md".to_string(),
            vec!["Chain break (bad code → migrate)".to_string()],
        ),
        QualityChecklist::new("cfg-scatter.checklist.md".to_string(), vec![]),
        QualityChecklist::new("modularity.checklist.md".to_string(), vec![]),
        QualityChecklist::new("visibility.checklist.md".to_string(), vec![]),
    ]);
    let items = source.items(&area).into_diagnostic()?;
    assert_eq!(items.len(), 6);
    assert_eq!(
        items[0].heading_trail(),
        &vec!["`crate`", "Library", "unwrap"]
    );
    assert_eq!(items[0].guidance().as_deref(), Some("Fix library aborts."));
    assert_eq!(items[0].detail_lines(), &["  - source: io::Error"]);
    assert_eq!(items[0].file().as_deref(), Some("src/store.rs"));
    assert!(items[1].item_line().contains("boxed"));
    assert!(items[2].item_line().contains("lost_source"));
    assert_eq!(items[3].file().as_deref(), Some("src/lib.rs"));
    assert_eq!(items[3].detail_lines(), &["  - sample: gated method"]);
    assert_eq!(items[4].file().as_deref(), Some("src/large.rs"));
    assert_eq!(
        items[4].detail_lines(),
        &["  - [ ] `method_a` — split this body"]
    );
    assert_eq!(items[5].file().as_deref(), Some("src/lib.rs"));
    Ok(())
}

#[test]
fn item_id_ignores_location_line_number() -> miette::Result<()> {
    cordial::init_tracing();
    let findings = tempfile::tempdir().into_diagnostic()?;
    let path = findings.path().join("panics.checklist.md");
    let source = ChecklistSource::from_findings_dir(findings.path());
    let area = area(vec![QualityChecklist::new(
        "panics.checklist.md".to_string(),
        vec![],
    )]);
    std::fs::write(&path, "## crate\n\n- [ ] `read_all` — `src/store.rs:52`\n")
        .into_diagnostic()?;
    let first = source.items(&area).into_diagnostic()?;
    std::fs::write(&path, "## crate\n\n- [ ] `read_all` — `src/store.rs:89`\n")
        .into_diagnostic()?;
    let second = source.items(&area).into_diagnostic()?;
    assert_eq!(first[0].id(), second[0].id());
    assert_eq!(first[0].scoring_key(), second[0].scoring_key());
    std::fs::write(
        &path,
        "## crate\n\n- [ ] `read_all` — `src/store.rs:89` — timeout:30\n",
    )
    .into_diagnostic()?;
    let third = source.items(&area).into_diagnostic()?;
    std::fs::write(
        &path,
        "## crate\n\n- [ ] `read_all` — `src/store.rs:89` — timeout:31\n",
    )
    .into_diagnostic()?;
    let fourth = source.items(&area).into_diagnostic()?;
    assert_ne!(third[0].id(), fourth[0].id());
    Ok(())
}

#[test]
fn worklist_build_checks_counts_and_writes_generations() -> miette::Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    let store = StoreLayout::from_root(root.path(), "fixture");
    std::fs::create_dir_all(store.findings_dir()).into_diagnostic()?;
    std::fs::write(
        store.findings_dir().join("panics.checklist.md"),
        "## crate\n\n- [ ] `read_all` — `src/store.rs:52`\n- [ ] `read_all` — `src/store.rs:89`\n",
    )
    .into_diagnostic()?;
    let selected = QualityAreaSummary::new(
        1,
        "Error handling".to_string(),
        2,
        "panics.checklist.md".to_string(),
        vec![QualityChecklist::new(
            "panics.checklist.md".to_string(),
            vec![],
        )],
        "summary.md".to_string(),
        String::new(),
    );
    let report = QualityReport::new(vec![selected.clone()], 2);
    let source = ChecklistSource::new(&store);
    let config = CordialConfig::default();
    let mut first = Worklist::build(&report, &store, &source, &config).into_diagnostic()?;
    assert_eq!(*first.record().generation(), 1);
    assert_eq!(first.open_count(), 2);
    assert!(first.current().is_none());
    assert_eq!(
        first.validity(&config).into_diagnostic()?,
        Validity::Current
    );
    assert!(first.items()[1].id().as_str().ends_with("-2"));
    assert!(first.activate());
    assert!(!first.activate());
    assert!(render_open(&first).contains(first.current().expect("current").id().as_str()));
    assert!(render_stats(&first, true, Some("Error handling"), false).contains("[>]"));
    first.write(&store).into_diagnostic()?;
    assert_eq!(
        Worklist::read(&store).into_diagnostic()?,
        Some(first.clone())
    );
    let second = Worklist::build(&report, &store, &source, &config).into_diagnostic()?;
    assert_eq!(*second.record().generation(), 2);
    let bad = QualityReport::new(
        vec![QualityAreaSummary::new(
            1,
            "Error handling".to_string(),
            1,
            "panics.checklist.md".to_string(),
            vec![QualityChecklist::new(
                "panics.checklist.md".to_string(),
                vec![],
            )],
            "summary.md".to_string(),
            String::new(),
        )],
        1,
    );
    assert!(Worklist::build(&bad, &store, &source, &config).is_err());
    Ok(())
}

#[test]
fn unchanged_config_with_maps_keeps_worklist_current() -> miette::Result<()> {
    cordial::init_tracing();
    let project = tempfile::tempdir().into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    std::fs::write(
        project.path().join("cordial.toml"),
        "[cfg_hygiene]\ncrate_verifier = { amenable_kani = 'kani', amenable_creusot = 'creusot', amenable_verus = 'verus' }\n[visibility]\nmod_thin_skip = { amenable_verus = ['gallery'], amenable_core = ['proof'] }\n",
    )
    .into_diagnostic()?;
    let config = load_cordial_config(project.path(), home.path());
    let store = StoreLayout::from_root(home.path().join("fixture"), "fixture");
    let list = Worklist::build(
        &QualityReport::new(vec![], 0),
        &store,
        &ChecklistSource::new(&store),
        &config,
    )
    .into_diagnostic()?;
    list.write(&store).into_diagnostic()?;
    let stored = Worklist::read(&store)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("worklist missing"))?;
    for _ in 0..32 {
        let reloaded = load_cordial_config(project.path(), home.path());
        assert_eq!(reloaded, config);
        assert_eq!(
            stored.validity(&reloaded).into_diagnostic()?,
            Validity::Current
        );
    }
    Ok(())
}

#[test]
fn verbose_stats_caps_items_unless_all_is_set() -> miette::Result<()> {
    cordial::init_tracing();
    let root = tempfile::tempdir().into_diagnostic()?;
    let store = StoreLayout::from_root(root.path(), "fixture");
    std::fs::create_dir_all(store.findings_dir()).into_diagnostic()?;
    let mut checklist = String::from("## crate\n\n");
    for index in 0..26 {
        checklist.push_str(&format!("- [ ] item {index} — `src/lib.rs:{index}`\n"));
    }
    std::fs::write(store.findings_dir().join("panics.checklist.md"), checklist)
        .into_diagnostic()?;
    let report = QualityReport::new(
        vec![QualityAreaSummary::new(
            1,
            "Error handling".to_string(),
            26,
            "panics.checklist.md".to_string(),
            vec![QualityChecklist::new(
                "panics.checklist.md".to_string(),
                vec![],
            )],
            "summary.md".to_string(),
            String::new(),
        )],
        26,
    );
    let list = Worklist::build(
        &report,
        &store,
        &ChecklistSource::new(&store),
        &CordialConfig::default(),
    )
    .into_diagnostic()?;
    let capped = render_stats(&list, true, None, false);
    assert!(capped.contains("Showing 25 of 26 items."));
    assert!(!capped.contains("item 25"));
    let all = render_stats(&list, true, None, true);
    assert!(all.contains("item 25"));
    assert!(!all.contains("Showing 25"));
    Ok(())
}
