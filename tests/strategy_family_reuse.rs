#![cfg(feature = "panics")]
//! Reuse selection consumes production family evidence without changing sources.

#[path = "support/strategy_specimens.rs"]
mod specimens;

use cordial::{
    ATTR_FUNCTION_ERROR_FAMILY, ErrorFamilyBasis, ErrorFamilyConflict,
    ErrorFamilyDisposition as Disposition, ErrorFamilyReuseBlocker as Blocker,
    ErrorFamilyReusePlan, ErrorFamilyReusePolicy, IrView, NodeId, ReusableErrorFamily,
    SourceErrorReuse, plan_error_family_reuse,
};
use miette::IntoDiagnostic;
use specimens::{build, copy_specimen, discover, execute, function_facts};
use std::{fs, num::NonZeroUsize};

fn fixture() -> miette::Result<(tempfile::TempDir, cordial::CrateIr)> {
    let fixture = copy_specimen("apply_failure_site")?;
    fs::write(
        fixture.path().join("src/main.rs"),
        include_str!("fixtures/strategy_family_reuse.rs.txt"),
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    Ok((fixture, ir))
}

fn node(ir: &dyn IrView, name: &str) -> miette::Result<NodeId> {
    ir.node_by_path(name)
        .ok_or_else(|| miette::miette!("missing {name}"))
}

fn group<'a>(
    ir: &dyn IrView,
    plan: &'a ErrorFamilyReusePlan,
    name: &str,
) -> miette::Result<&'a ReusableErrorFamily> {
    let id = node(ir, name)?;
    plan.groups()
        .iter()
        .find(|group| group.functions().contains(&id))
        .ok_or_else(|| miette::miette!("missing group for {name}"))
}

fn policy(minimum: NonZeroUsize, reuse: SourceErrorReuse) -> ErrorFamilyReusePolicy {
    ErrorFamilyReusePolicy::new(ErrorFamilyBasis::PanicMigration, minimum, reuse)
}

fn scope(ir: &dyn IrView) -> miette::Result<Vec<NodeId>> {
    [
        "a",
        "a_again",
        "a_third",
        "b",
        "b_again",
        "b_third",
        "domain",
        "domain_again",
        "domain_third",
    ]
    .into_iter()
    .map(|name| node(ir, name))
    .collect()
}

#[test]
fn exact_sets_count_distinct_functions_and_preserve_domain_origins() -> miette::Result<()> {
    cordial::init_tracing();
    let (fixture, ir) = fixture()?;
    build(fixture.path())?;
    assert!(execute(fixture.path(), &[])?.status.success());
    let source = fs::read(fixture.path().join("src/main.rs")).into_diagnostic()?;
    let mut functions = scope(&ir)?;
    functions.extend([node(&ir, "a")?, node(&ir, "a_third")?]);
    let minimum = NonZeroUsize::new(3).ok_or_else(|| miette::miette!("threshold"))?;
    let plan = plan_error_family_reuse(
        &ir,
        &functions,
        policy(minimum, SourceErrorReuse::Permitted),
    )
    .into_diagnostic()?;
    assert!(plan.issues().is_empty());
    assert_eq!(plan.groups().len(), 3);
    for candidate in plan.groups() {
        assert_eq!(candidate.functions().len(), 3);
        assert_eq!(candidate.disposition(), Disposition::Selected);
    }
    let a = group(&ir, &plan, "a")?;
    assert_eq!(function_facts(&ir, "a_third")?.calls().len(), 2);
    let domain = group(&ir, &plan, "domain")?;
    assert_eq!(a.members()[0].identity(), domain.members()[0].identity());
    assert_ne!(a.members()[0].origins(), domain.members()[0].origins());
    assert_eq!(a.members()[1], domain.members()[1]);
    functions.reverse();
    assert_eq!(
        plan,
        plan_error_family_reuse(
            &ir,
            &functions,
            policy(minimum, SourceErrorReuse::Permitted)
        )
        .into_diagnostic()?
    );
    let four = NonZeroUsize::new(4).ok_or_else(|| miette::miette!("threshold"))?;
    let below = plan_error_family_reuse(&ir, &functions, policy(four, SourceErrorReuse::Permitted))
        .into_diagnostic()?;
    assert!(
        below
            .groups()
            .iter()
            .all(|candidate| candidate.disposition() == Disposition::BelowThreshold)
    );
    assert_eq!(
        fs::read(fixture.path().join("src/main.rs")).into_diagnostic()?,
        source
    );
    Ok(())
}

#[test]
fn no_reuse_reports_crossings_and_distinct_producers_without_broadening() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let minimum = NonZeroUsize::new(3).ok_or_else(|| miette::miette!("threshold"))?;
    let permitted = plan_error_family_reuse(
        &ir,
        &scope(&ir)?,
        policy(minimum, SourceErrorReuse::Permitted),
    )
    .into_diagnostic()?;
    let forbidden = plan_error_family_reuse(
        &ir,
        &scope(&ir)?,
        policy(minimum, SourceErrorReuse::Forbidden),
    )
    .into_diagnostic()?;
    assert_eq!(
        group(&ir, &forbidden, "a")?.disposition(),
        Disposition::Selected
    );
    for (name, reason) in [
        ("b", ErrorFamilyConflict::CrossingOverlap),
        ("domain", ErrorFamilyConflict::DistinctProducers),
    ] {
        let candidate = group(&ir, &forbidden, name)?;
        let Disposition::Overlap {
            selected_group,
            conflict,
        } = candidate.disposition()
        else {
            return Err(miette::miette!("expected overlap for {name}"));
        };
        assert_eq!(conflict, reason);
        assert_eq!(
            forbidden.groups()[selected_group].disposition(),
            Disposition::Selected
        );
        assert_eq!(candidate.members(), group(&ir, &permitted, name)?.members());
    }
    Ok(())
}

#[test]
fn consistent_subset_families_can_nest_but_crossing_siblings_cannot() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let mut functions = scope(&ir)?;
    for name in [
        "nested",
        "nested_again",
        "nested_third",
        "single",
        "single_again",
        "single_third",
    ] {
        functions.push(node(&ir, name)?);
    }
    let minimum = NonZeroUsize::new(3).ok_or_else(|| miette::miette!("threshold"))?;
    let plan = plan_error_family_reuse(
        &ir,
        &functions,
        policy(minimum, SourceErrorReuse::Forbidden),
    )
    .into_diagnostic()?;
    for name in ["nested", "a", "single"] {
        assert_eq!(
            group(&ir, &plan, name)?.disposition(),
            Disposition::Selected
        );
    }
    assert_eq!(plan.groups()[0].members().len(), 3);
    assert!(matches!(
        group(&ir, &plan, "b")?.disposition(),
        Disposition::Overlap {
            conflict: ErrorFamilyConflict::CrossingOverlap,
            ..
        }
    ));
    Ok(())
}

#[test]
fn disjoint_families_survive_and_more_frequent_contracts_win() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let minimum = NonZeroUsize::new(3).ok_or_else(|| miette::miette!("threshold"))?;
    let mut functions = scope(&ir)?;
    for name in ["utf8_only", "utf8_again", "utf8_third"] {
        functions.push(node(&ir, name)?);
    }
    let plan = plan_error_family_reuse(
        &ir,
        &functions,
        policy(minimum, SourceErrorReuse::Forbidden),
    )
    .into_diagnostic()?;
    for name in ["a", "utf8_only"] {
        assert_eq!(
            group(&ir, &plan, name)?.disposition(),
            Disposition::Selected
        );
    }
    functions.push(node(&ir, "b_fourth")?);
    let frequent = plan_error_family_reuse(
        &ir,
        &functions,
        policy(minimum, SourceErrorReuse::Forbidden),
    )
    .into_diagnostic()?;
    assert_eq!(frequent.groups()[0].functions().len(), 4);
    assert_eq!(
        group(&ir, &frequent, "b")?.disposition(),
        Disposition::Selected
    );
    assert!(matches!(
        group(&ir, &frequent, "a")?.disposition(),
        Disposition::Overlap {
            conflict: ErrorFamilyConflict::CrossingOverlap,
            ..
        }
    ));
    Ok(())
}

#[test]
fn member_order_is_irrelevant_but_duplicate_evidence_is_invalid() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let one = NonZeroUsize::new(1).ok_or_else(|| miette::miette!("threshold"))?;
    let functions = [node(&ir, "a")?, node(&ir, "two_producers")?];
    let preview =
        plan_error_family_reuse(&ir, &functions, policy(one, SourceErrorReuse::Permitted))
            .into_diagnostic()?;
    let mut reordered = ir.clone();
    for function in functions {
        let mut family = serde_json::to_value(
            ir.function_error_family(function)
                .into_diagnostic()?
                .ok_or_else(|| miette::miette!("family"))?,
        )
        .into_diagnostic()?;
        let members = family["members"]
            .as_array_mut()
            .ok_or_else(|| miette::miette!("members"))?;
        members.reverse();
        for member in members {
            member["origins"]
                .as_array_mut()
                .ok_or_else(|| miette::miette!("origins"))?
                .reverse();
        }
        reordered
            .set_attr(function, ATTR_FUNCTION_ERROR_FAMILY, family)
            .into_diagnostic()?;
    }
    assert_eq!(
        preview,
        plan_error_family_reuse(
            &reordered,
            &functions,
            policy(one, SourceErrorReuse::Permitted)
        )
        .into_diagnostic()?
    );
    let mut duplicate = serde_json::to_value(
        ir.function_error_family(functions[1])
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("family"))?,
    )
    .into_diagnostic()?;
    let origin = duplicate["members"][0]["origins"][0].clone();
    duplicate["members"][0]["origins"]
        .as_array_mut()
        .ok_or_else(|| miette::miette!("origins"))?
        .push(origin);
    reordered
        .set_attr(functions[1], ATTR_FUNCTION_ERROR_FAMILY, duplicate)
        .into_diagnostic()?;
    let invalid = plan_error_family_reuse(
        &reordered,
        &functions[1..],
        policy(one, SourceErrorReuse::Permitted),
    )
    .into_diagnostic()?;
    assert!(invalid.groups().is_empty());
    assert_eq!(invalid.issues()[0].reason(), Blocker::InvalidEvidence);
    Ok(())
}

#[test]
fn gaps_staleness_and_invalid_producers_never_inflate_reuse() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let one = NonZeroUsize::new(1).ok_or_else(|| miette::miette!("threshold"))?;
    let functions = [
        node(&ir, "a")?,
        node(&ir, "unknown")?,
        node(&ir, "io")?,
        NodeId::new(u32::MAX),
        node(&ir, "empty")?,
    ];
    let plan = plan_error_family_reuse(&ir, &functions, policy(one, SourceErrorReuse::Permitted))
        .into_diagnostic()?;
    for (id, reason) in [
        (functions[1], Blocker::Incomplete),
        (functions[2], Blocker::DifferentBasis),
        (functions[3], Blocker::MissingFunction),
    ] {
        assert!(
            plan.issues()
                .iter()
                .any(|issue| issue.function() == id && issue.reason() == reason)
        );
    }
    assert_eq!(group(&ir, &plan, "a")?.functions(), &[functions[0]]);
    assert_eq!(
        group(&ir, &plan, "empty")?.disposition(),
        Disposition::Infallible
    );
    let original = serde_json::to_value(
        ir.function_error_family(functions[0])
            .into_diagnostic()?
            .ok_or_else(|| miette::miette!("family"))?,
    )
    .into_diagnostic()?;
    for (field, value) in [
        ("source_digest", serde_json::json!(vec![0; 32])),
        (
            "members",
            serde_json::json!([{"identity":"::std::io::Error", "origins":[functions[0]]}]),
        ),
    ] {
        let mut changed = ir.clone();
        let mut family = original.clone();
        family[field] = value;
        changed
            .set_attr(functions[0], ATTR_FUNCTION_ERROR_FAMILY, family)
            .into_diagnostic()?;
        let result = plan_error_family_reuse(
            &changed,
            &functions[..1],
            policy(one, SourceErrorReuse::Permitted),
        )
        .into_diagnostic()?;
        assert!(result.groups().is_empty());
        assert_eq!(result.issues()[0].reason(), Blocker::InvalidEvidence);
    }
    let mut corrupt = ir.clone();
    corrupt
        .set_attr(
            functions[0],
            ATTR_FUNCTION_ERROR_FAMILY,
            serde_json::json!({"broken":true}),
        )
        .into_diagnostic()?;
    assert!(
        plan_error_family_reuse(
            &corrupt,
            &functions[..1],
            policy(one, SourceErrorReuse::Permitted)
        )
        .is_err()
    );
    Ok(())
}
