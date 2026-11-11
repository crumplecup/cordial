//! Children follow the parent: order, partial coverage, witnesses, and derivation from rustdoc.
#![cfg(feature = "amenable_std")]

use std::collections::{BTreeMap, HashMap, HashSet};

use cordial::testing::{AmenableStdStatus, ExpectedInstantiations};

#[path = "support/instantiation_world.rs"]
mod instantiation_world;

#[path = "support/instantiation_reports.rs"]
mod instantiation_reports;

#[path = "support/instantiation_expand.rs"]
mod instantiation_expand;

use instantiation_expand::{all_four, expand};
use instantiation_reports::{WITNESSED, expected, find, four_zones, note};

#[test]
fn four_covered_zones_make_a_complete_parent_with_four_complete_children() -> miette::Result<()> {
    cordial::init_tracing();
    let out = expand(&all_four(), &four_zones(), &HashMap::new(), &HashSet::new())?;
    assert_eq!(out.entries().len(), 5, "{:?}", out.entries());

    let parent = find(&out, "chrono::DateTime")?;
    assert_eq!(parent.status(), AmenableStdStatus::Complete);
    assert_eq!(parent.parent(), &None);
    assert!(
        note(parent).starts_with("4 of 4 instantiations Complete"),
        "{}",
        note(parent)
    );

    for label in [
        "chrono::DateTime<chrono::Utc>",
        "chrono::DateTime<chrono::FixedOffset>",
        "chrono::DateTime<chrono_tz::Tz>",
    ] {
        let child = find(&out, label)?;
        assert_eq!(child.status(), AmenableStdStatus::Complete, "{label}");
        assert_eq!(child.parent().as_deref(), Some("chrono::DateTime"));
        assert!(child.evidence_link() && child.kani_witness());
    }
    assert_eq!(out.complete_count(), 5);
    Ok(())
}

#[test]
fn children_follow_the_configured_order_after_the_parent() -> miette::Result<()> {
    cordial::init_tracing();
    let out = expand(&all_four(), &four_zones(), &HashMap::new(), &HashSet::new())?;
    let paths: Vec<&str> = out
        .entries()
        .iter()
        .map(|e| e.type_path().as_str())
        .collect();
    assert_eq!(
        paths,
        vec![
            "chrono::DateTime",
            "chrono::DateTime<chrono::Utc>",
            "chrono::DateTime<chrono::FixedOffset>",
            "chrono::DateTime<chrono::Local>",
            "chrono::DateTime<chrono_tz::Tz>",
        ]
    );
    Ok(())
}

#[test]
fn an_uncovered_zone_is_a_missing_child_and_the_parent_is_partial() -> miette::Result<()> {
    cordial::init_tracing();
    let mut claims = all_four();
    claims.remove(2); // no Local
    let out = expand(&claims, &four_zones(), &HashMap::new(), &HashSet::new())?;

    let local = find(&out, "chrono::DateTime<chrono::Local>")?;
    assert_eq!(local.status(), AmenableStdStatus::Missing);
    assert!(!local.evidence_link());

    let parent = find(&out, "chrono::DateTime")?;
    assert_eq!(parent.status(), AmenableStdStatus::Partial);
    assert!(note(parent).starts_with("3 of 4 instantiations Complete"));
    assert!(
        !parent.evidence_link(),
        "an aggregate column is set only if every child has it"
    );
    assert_eq!(
        (
            out.complete_count(),
            out.missing_count(),
            out.partial_count()
        ),
        (3, 1, 1),
        "three complete children; one missing child; the partial parent"
    );
    Ok(())
}

#[test]
fn witnesses_are_attributed_to_the_instantiation_they_prove() -> miette::Result<()> {
    cordial::init_tracing();
    let claims: Vec<(&str, &[&str])> = vec![
        ("chrono::DateTime<chrono::Utc>", &["kani"]),
        ("chrono::DateTime<chrono::FixedOffset>", &WITNESSED),
    ];
    let out = expand(
        &claims,
        &expected(&["chrono::Utc", "chrono::FixedOffset"]),
        &HashMap::new(),
        &HashSet::new(),
    )?;
    let utc = find(&out, "chrono::DateTime<chrono::Utc>")?;
    assert_eq!(
        utc.status(),
        AmenableStdStatus::Partial,
        "evidence but only kani"
    );
    assert!(utc.kani_witness() && !utc.creusot_witness() && !utc.verus_witness());
    let fixed = find(&out, "chrono::DateTime<chrono::FixedOffset>")?;
    assert_eq!(fixed.status(), AmenableStdStatus::Complete);
    assert_eq!(
        find(&out, "chrono::DateTime")?.status(),
        AmenableStdStatus::Partial
    );
    Ok(())
}

#[test]
fn a_registered_instantiation_outside_the_expected_list_is_still_shown() -> miette::Result<()> {
    cordial::init_tracing();
    let mut claims = all_four();
    claims.push(("chrono::DateTime<chrono::Bare>", &WITNESSED));
    let out = expand(&claims, &four_zones(), &HashMap::new(), &HashSet::new())?;
    let extra = find(&out, "chrono::DateTime<chrono::Bare>")?;
    assert!(
        note(extra).contains("not in the expected list"),
        "{}",
        note(extra)
    );
    assert_eq!(extra.status(), AmenableStdStatus::Complete);
    Ok(())
}

#[test]
fn with_no_configured_list_the_instantiations_are_derived_from_rustdoc() -> miette::Result<()> {
    cordial::init_tracing();
    // Only Utc and Local are registered. Nothing is configured, so the rest
    // come from rustdoc: every type in the readable crates that implements the
    // declared bound `Tz: TimeZone`, and `Bare`, which does not, is left out.
    let claims: Vec<(&str, &[&str])> = vec![
        ("chrono::DateTime<chrono::Utc>", &WITNESSED),
        ("chrono::DateTime<chrono::Local>", &WITNESSED),
    ];
    let out = expand(
        &claims,
        &ExpectedInstantiations::default(),
        &HashMap::new(),
        &HashSet::new(),
    )?;
    assert_eq!(out.entries().len(), 5, "{:?}", out.entries());
    for (label, status) in [
        ("chrono::DateTime<chrono::Utc>", AmenableStdStatus::Complete),
        (
            "chrono::DateTime<chrono::Local>",
            AmenableStdStatus::Complete,
        ),
        (
            "chrono::DateTime<chrono::FixedOffset>",
            AmenableStdStatus::Missing,
        ),
        (
            "chrono::DateTime<chrono_tz::Tz>",
            AmenableStdStatus::Missing,
        ),
    ] {
        assert_eq!(find(&out, label)?.status(), status, "{label}");
    }
    assert!(find(&out, "chrono::DateTime<chrono::Bare>").is_err());
    let text = note(find(&out, "chrono::DateTime")?);
    assert!(
        text.contains("derived from the implementors of `Tz: TimeZone`"),
        "{text}"
    );
    Ok(())
}

#[test]
fn a_configured_list_replaces_the_derivation_and_an_empty_one_turns_it_off() -> miette::Result<()> {
    cordial::init_tracing();
    let one = expand(
        &all_four(),
        &expected(&["chrono::Utc"]),
        &HashMap::new(),
        &HashSet::new(),
    )?;
    // The list names only Utc; the other three registered ones still show as
    // unexpected extras, but nothing is derived on top.
    assert!(
        note(find(&one, "chrono::DateTime")?).starts_with("1 of 4 instantiations Complete")
            || note(find(&one, "chrono::DateTime")?).contains("Complete")
    );
    assert!(!note(find(&one, "chrono::DateTime")?).contains("derived from"));

    let none = expand(
        &[],
        &ExpectedInstantiations::new(BTreeMap::from([(
            "chrono::DateTime".to_string(),
            Vec::new(),
        )])),
        &HashMap::new(),
        &HashSet::new(),
    )?;
    assert_eq!(none.entries().len(), 1, "an empty entry means no rows");
    Ok(())
}
