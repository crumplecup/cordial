#![cfg(feature = "amenable_ext")]

//! How per-instantiation rows travel: through findings, into the gaps list,
//! and out as CSV and the markdown checklist.

use std::collections::HashMap;

use miette::IntoDiagnostic;

use cordial::testing::{
    AmenableStdEntry, AmenableStdReport, AmenableStdStatus, ExtRowFinding, ExtRowRule,
    build_amenable_ext_gaps, ext_gaps_from_findings, ext_report_from_findings, ext_row_disposition,
    render_amenable_ext_checklist_md,
};
use cordial::{CrateIr, Finding, NodeAnchor};

fn entry(
    path: &str,
    status: AmenableStdStatus,
    parent: Option<&str>,
    note: Option<&str>,
    instantiations: usize,
) -> miette::Result<AmenableStdEntry> {
    let complete = status == AmenableStdStatus::Complete;
    AmenableStdEntry::builder()
        .type_path(path.to_string())
        .type_kind("struct".to_string())
        .is_generic(instantiations > 0)
        .evidence_link(complete)
        .evidence_name(None)
        .kani_witness(complete)
        .creusot_witness(complete)
        .verus_witness(complete)
        .proof_test(false)
        .status(status)
        .skip_reason(None)
        .kani_excepted(false)
        .creusot_excepted(false)
        .verus_excepted(false)
        .parent(parent.map(str::to_string))
        .note(note.map(str::to_string))
        .instantiations(instantiations)
        .build()
        .into_diagnostic()
}

/// A plain row, an aggregate with three instantiations: Utc Complete, Local
/// Missing, Tz Partial.
fn sample_report() -> miette::Result<AmenableStdReport> {
    let parent = "chrono::DateTime";
    let entries = vec![
        entry("chrono::Utc", AmenableStdStatus::Complete, None, None, 0)?,
        entry(
            parent,
            AmenableStdStatus::Partial,
            None,
            Some(
                "1 of 3 instantiations Complete. declared bounds `Tz: TimeZone`: satisfied by Utc, Local, Tz",
            ),
            3,
        )?,
        entry(
            "chrono::DateTime<chrono::Utc>",
            AmenableStdStatus::Complete,
            Some(parent),
            None,
            0,
        )?,
        entry(
            "chrono::DateTime<chrono::Local>",
            AmenableStdStatus::Missing,
            Some(parent),
            None,
            0,
        )?,
        entry(
            "chrono::DateTime<chrono_tz::Tz>",
            AmenableStdStatus::Partial,
            Some(parent),
            None,
            0,
        )?,
    ];
    AmenableStdReport::builder()
        .source_crate("chrono".to_string())
        .impl_crate("amenable_ext".to_string())
        .include_nightly(false)
        .entries(entries)
        .complete_count(2)
        .partial_count(2)
        .missing_count(1)
        .skipped_count(0)
        .build()
        .into_diagnostic()
}

#[test]
fn entries_know_their_kind() -> miette::Result<()> {
    cordial::init_tracing();
    let report = sample_report()?;
    let kinds: Vec<&str> = report.entries().iter().map(|e| e.kind()).collect();
    assert_eq!(
        kinds,
        [
            "plain",
            "aggregate",
            "instantiation",
            "instantiation",
            "instantiation"
        ]
    );
    Ok(())
}

#[test]
fn the_checklist_shows_instantiations_under_their_generic_type() -> miette::Result<()> {
    cordial::init_tracing();
    let md =
        render_amenable_ext_checklist_md(&sample_report()?, &HashMap::new(), "amenable_ext_chrono")
            .into_diagnostic()?;

    // The action lists carry the instantiation rows, not the aggregate.
    let missing = md
        .split("## Partial witness coverage")
        .next()
        .unwrap_or_default();
    assert!(
        missing.contains(
            "- [ ] `chrono::DateTime<chrono::Local>` (struct) (instantiation of `chrono::DateTime`)"
        ),
        "{md}"
    );
    assert!(!missing.contains("- [ ] `chrono::DateTime` "), "{md}");
    let partial = md
        .split("## Partial witness coverage")
        .nth(1)
        .and_then(|rest| rest.split("## Generic types").next())
        .unwrap_or_default();
    assert!(
        partial.contains("`chrono::DateTime<chrono_tz::Tz>` (instantiation of `chrono::DateTime`)"),
        "{md}"
    );
    assert!(!partial.contains("- [ ] `chrono::DateTime` —"), "{md}");

    // Its own section gives the whole picture, with the note.
    let section = md
        .split("## Generic types and their instantiations (1)")
        .nth(1)
        .and_then(|rest| rest.split("## Documented exceptions").next())
        .unwrap_or_default();
    assert!(section.contains("### `chrono::DateTime` — Partial"), "{md}");
    assert!(section.contains("1 of 3 instantiations Complete"), "{md}");
    assert!(
        section.contains("- `chrono::DateTime<chrono::Utc>` — Complete"),
        "{md}"
    );
    assert!(
        section.contains("- `chrono::DateTime<chrono::Local>` — Missing"),
        "{md}"
    );
    Ok(())
}

#[test]
fn a_report_without_generics_has_no_generic_section() -> miette::Result<()> {
    cordial::init_tracing();
    let report = AmenableStdReport::builder()
        .source_crate("jiff".to_string())
        .impl_crate("amenable_ext".to_string())
        .include_nightly(false)
        .entries(vec![entry(
            "jiff::Zoned",
            AmenableStdStatus::Complete,
            None,
            None,
            0,
        )?])
        .complete_count(1)
        .partial_count(0)
        .missing_count(0)
        .skipped_count(0)
        .build()
        .into_diagnostic()?;
    let md = render_amenable_ext_checklist_md(&report, &HashMap::new(), "amenable_ext_jiff")
        .into_diagnostic()?;
    assert!(
        !md.contains("Generic types and their instantiations"),
        "{md}"
    );
    Ok(())
}

#[test]
fn gaps_list_the_instantiations_and_leave_the_aggregate_out() -> miette::Result<()> {
    cordial::init_tracing();
    let gaps = build_amenable_ext_gaps(&sample_report()?);
    let paths: Vec<&str> = gaps.iter().map(|g| g.type_path().as_str()).collect();
    assert_eq!(
        paths,
        [
            "chrono::DateTime<chrono::Local>",
            "chrono::DateTime<chrono_tz::Tz>"
        ]
    );
    Ok(())
}

// ---- findings ---------------------------------------------------------------

fn finding(entry: &AmenableStdEntry, anchor: NodeAnchor) -> miette::Result<ExtRowFinding> {
    ExtRowFinding::builder()
        .rule(ExtRowRule::new("chrono"))
        .disposition(ext_row_disposition(entry.status()))
        .anchor(anchor)
        .source_crate("chrono".to_string())
        .impl_crate("amenable_ext".to_string())
        .type_path(entry.type_path().clone())
        .type_kind(entry.type_kind().clone())
        .is_generic(entry.is_generic())
        .status(entry.status())
        .evidence_link(entry.evidence_link())
        .evidence_name(entry.evidence_name().clone())
        .kani_witness(entry.kani_witness())
        .creusot_witness(entry.creusot_witness())
        .verus_witness(entry.verus_witness())
        .proof_test(entry.proof_test())
        .skip_reason(entry.skip_reason().clone())
        .kani_excepted(entry.kani_excepted())
        .creusot_excepted(entry.creusot_excepted())
        .verus_excepted(entry.verus_excepted())
        .missing_layers(String::new())
        .action("act".to_string())
        .parent(entry.parent().clone())
        .note(entry.note().clone())
        .instantiations(entry.instantiations())
        .build()
        .into_diagnostic()
}

#[test]
fn parent_note_and_count_survive_the_trip_through_findings() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = CrateIr::new("demo");
    let anchor = NodeAnchor::new(ir.root());
    let original = sample_report()?;
    let findings: Vec<ExtRowFinding> = original
        .entries()
        .iter()
        .map(|entry| finding(entry, anchor))
        .collect::<miette::Result<_>>()?;
    let refs: Vec<&dyn Finding> = findings.iter().map(|f| f as &dyn Finding).collect();

    let rebuilt = ext_report_from_findings(&refs, "amenable-ext-chrono", false)
        .into_diagnostic()?
        .ok_or_else(|| miette::miette!("no report"))?;
    assert_eq!(rebuilt.entries().len(), original.entries().len());
    for (before, after) in original.entries().iter().zip(rebuilt.entries()) {
        assert_eq!(before.parent(), after.parent(), "{}", before.type_path());
        assert_eq!(before.note(), after.note(), "{}", before.type_path());
        assert_eq!(before.instantiations(), after.instantiations());
        assert_eq!(before.kind(), after.kind());
    }
    Ok(())
}

#[test]
fn open_aggregate_findings_are_not_gaps_but_open_instantiations_are() -> miette::Result<()> {
    cordial::init_tracing();
    let ir = CrateIr::new("demo");
    let anchor = NodeAnchor::new(ir.root());
    let findings: Vec<ExtRowFinding> = sample_report()?
        .entries()
        .iter()
        .map(|entry| finding(entry, anchor))
        .collect::<miette::Result<_>>()?;
    let refs: Vec<&dyn Finding> = findings.iter().map(|f| f as &dyn Finding).collect();
    let gaps = ext_gaps_from_findings(&refs, "amenable-ext-chrono");
    let paths: Vec<&str> = gaps.iter().map(|g| g.type_path().as_str()).collect();
    assert_eq!(
        paths,
        [
            "chrono::DateTime<chrono::Local>",
            "chrono::DateTime<chrono_tz::Tz>"
        ]
    );
    Ok(())
}

// ---- configuration ----------------------------------------------------------

#[test]
fn amenable_ext_keys_load_from_cordial_toml() -> miette::Result<()> {
    cordial::init_tracing();
    let dir = tempfile::tempdir().into_diagnostic()?;
    std::fs::write(
        dir.path().join("cordial.toml"),
        r#"
[amenable_ext]
derive_cap = 4

[[amenable_ext.target]]
name = "chrono"
resolve_crates = ["chrono_tz"]
instantiations = { "chrono::Date" = [["chrono::Utc"]], "chrono::NaiveDate" = [] }
"#,
    )
    .into_diagnostic()?;
    let home = tempfile::tempdir().into_diagnostic()?;
    let config = cordial::load_cordial_config(dir.path(), home.path());
    let ext = config.amenable_ext();
    assert_eq!(ext.derive_cap(), 4);
    assert_eq!(ext.alias_depth(), 8);
    assert_eq!(ext.max_type_nodes(), 64);
    let target = ext
        .target("chrono")
        .ok_or_else(|| miette::miette!("no target"))?;
    assert_eq!(target.resolve_crates(), &["chrono_tz".to_string()]);
    assert_eq!(
        target.instantiations().get("chrono::Date"),
        Some(&vec![vec!["chrono::Utc".to_string()]])
    );
    assert_eq!(
        target.instantiations().get("chrono::NaiveDate"),
        Some(&Vec::new())
    );
    Ok(())
}

#[test]
fn a_crate_name_resolves_to_the_package_cargo_knows() -> miette::Result<()> {
    cordial::init_tracing();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for spelling in ["tracing_subscriber", "tracing-subscriber"] {
        let package = cordial::testing::member_dependency_package_name(root, "cordial", spelling)
            .into_diagnostic()?;
        assert_eq!(package, "tracing-subscriber", "{spelling}");
    }
    assert!(
        cordial::testing::member_dependency_package_name(root, "cordial", "no_such_crate").is_err()
    );
    Ok(())
}
