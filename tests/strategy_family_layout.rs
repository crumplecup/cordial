#![cfg(feature = "panics")]
//! Declaration layouts retain consumer mappings and expand to exact alternatives.

#[path = "support/strategy_specimens.rs"]
mod specimens;

use cordial::{
    ErrorContractBroadening as Broadening, ErrorFamilyBasis, ErrorFamilyLayoutDecision as Decision,
    ErrorFamilyLayoutPlan, ErrorFamilyMember, ErrorFamilyReusePolicy, IrView, NodeId,
    SourceErrorReuse, plan_error_family_layout,
};
use miette::IntoDiagnostic;
use specimens::{build, copy_specimen, discover, execute, function_facts};
use std::{collections::BTreeMap, fs, num::NonZeroUsize};

fn fixture() -> miette::Result<(tempfile::TempDir, cordial::CrateIr)> {
    let fixture = copy_specimen("apply_failure_site")?;
    fs::write(
        fixture.path().join("src/main.rs"),
        include_str!("fixtures/strategy_family_representation.rs.txt"),
    )
    .into_diagnostic()?;
    let ir = discover(fixture.path())?;
    Ok((fixture, ir))
}

fn scope(ir: &dyn IrView, names: &[&str]) -> miette::Result<Vec<NodeId>> {
    names
        .iter()
        .map(|name| {
            ir.node_by_path(name)
                .ok_or_else(|| miette::miette!("missing {name}"))
        })
        .collect()
}

fn policy(minimum: usize, reuse: SourceErrorReuse) -> miette::Result<ErrorFamilyReusePolicy> {
    Ok(ErrorFamilyReusePolicy::new(
        ErrorFamilyBasis::PanicMigration,
        NonZeroUsize::new(minimum).ok_or_else(|| miette::miette!("threshold"))?,
        reuse,
    ))
}

fn ready(decision: Decision) -> miette::Result<ErrorFamilyLayoutPlan> {
    let Decision::Ready(layout) = decision else {
        return Err(miette::miette!("unexpected refusal: {decision:?}"));
    };
    Ok(layout)
}

fn family(ir: &dyn IrView, layout: &ErrorFamilyLayoutPlan, name: &str) -> miette::Result<usize> {
    let id = scope(ir, &[name])?[0];
    let group = layout
        .representation()
        .reuse()
        .groups()
        .iter()
        .position(|group| group.functions().contains(&id))
        .ok_or_else(|| miette::miette!("missing {name}"))?;
    layout.contract_families()[group].ok_or_else(|| miette::miette!("empty {name}"))
}

fn expanded(layout: &ErrorFamilyLayoutPlan, parent: usize) -> Vec<ErrorFamilyMember> {
    let family = &layout.families()[parent];
    let mut members = family.direct_members().clone();
    for child in family.children() {
        assert!(*child < parent, "acyclic declaration order");
        assert!(layout.families()[*child].members().len() < family.members().len());
        members.extend(expanded(layout, *child));
    }
    members.sort_by(|left, right| left.identity().cmp(right.identity()));
    members
}

fn assert_layout(layout: &ErrorFamilyLayoutPlan) {
    let representation = layout.representation();
    assert!(representation.blockers().is_empty());
    assert_eq!(
        layout.contract_families().len(),
        representation.contracts().len()
    );
    let mut direct_owners = BTreeMap::new();
    let mut parents = vec![0; layout.families().len()];
    for (index, family) in layout.families().iter().enumerate() {
        // Compare the complete expanded payload, not only counts or ids. No
        // deduplication here: duplicated alternatives must fail the comparison.
        assert_eq!(expanded(layout, index), *family.members());
        for member in family.direct_members() {
            *direct_owners.entry(member.identity()).or_insert(0) += 1;
        }
        for child in family.children() {
            parents[*child] += 1;
        }
        let mut consumers = 0;
        for group in family.exact_groups() {
            assert_eq!(layout.contract_families()[*group], Some(index));
            assert_eq!(
                representation.contracts()[*group].proposed_members(),
                family.members()
            );
            consumers += representation.reuse().groups()[*group].functions().len();
        }
        assert_eq!(consumers, family.function_count());
    }
    for (group, node) in layout.contract_families().iter().enumerate() {
        match node {
            Some(node) => assert!(layout.families()[*node].exact_groups().contains(&group)),
            None => assert!(
                representation.contracts()[group]
                    .proposed_members()
                    .is_empty()
            ),
        }
    }
    if representation.reuse_policy().source_reuse() == SourceErrorReuse::Forbidden {
        assert!(direct_owners.values().all(|count| *count == 1));
        assert!(parents.iter().all(|count| *count <= 1));
    }
}

#[test]
fn no_reuse_deduplicates_broadened_contracts_into_one_source_owner_tree() -> miette::Result<()> {
    cordial::init_tracing();
    let (fixture, ir) = fixture()?;
    build(fixture.path())?;
    assert!(execute(fixture.path(), &[])?.status.success());
    assert_eq!(function_facts(&ir, "a_third")?.calls().len(), 2);
    let source = fs::read(fixture.path().join("src/main.rs")).into_diagnostic()?;
    let mut functions = scope(
        &ir,
        &[
            "a", "a_again", "a_third", "b", "b_again", "b_third", "nested", "single", "empty",
        ],
    )?;
    let reuse = policy(3, SourceErrorReuse::Forbidden)?;
    let layout = ready(
        plan_error_family_layout(&ir, &functions, reuse, Broadening::Permitted)
            .into_diagnostic()?,
    )?;
    assert_layout(&layout);
    assert_eq!(layout.families().len(), 3);
    let a = family(&ir, &layout, "a")?;
    let b = family(&ir, &layout, "b")?;
    let single = family(&ir, &layout, "single")?;
    assert_eq!(b, family(&ir, &layout, "nested")?);
    assert_eq!(layout.families()[b].exact_groups().len(), 2);
    assert_eq!(layout.families()[b].function_count(), 4);
    assert_eq!(layout.families()[b].children(), &[a]);
    assert_eq!(layout.families()[a].children(), &[single]);
    assert_eq!(
        layout.families()[b].direct_members()[0].identity(),
        "::std::str::Utf8Error"
    );
    assert_eq!(
        layout.families()[a].direct_members()[0].identity(),
        "::std::num::ParseIntError"
    );
    assert_eq!(
        layout.families()[single].direct_members()[0].origins(),
        &scope(&ir, &["io"])?
    );
    functions.reverse();
    assert_eq!(
        Decision::Ready(layout),
        plan_error_family_layout(&ir, &functions, reuse, Broadening::Permitted)
            .into_diagnostic()?
    );
    assert_eq!(
        fs::read(fixture.path().join("src/main.rs")).into_diagnostic()?,
        source
    );
    Ok(())
}

#[test]
fn permitted_overlap_keeps_exact_returns_without_duplicating_a_source_in_one_enum()
-> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let functions = scope(
        &ir,
        &[
            "a", "a_again", "a_third", "b", "b_again", "b_third", "nested", "single",
        ],
    )?;
    let layout = ready(
        plan_error_family_layout(
            &ir,
            &functions,
            policy(3, SourceErrorReuse::Permitted)?,
            Broadening::Forbidden,
        )
        .into_diagnostic()?,
    )?;
    assert_layout(&layout);
    let a = family(&ir, &layout, "a")?;
    let b = family(&ir, &layout, "b")?;
    let root = family(&ir, &layout, "nested")?;
    assert_ne!(a, b);
    assert_eq!(layout.families()[root].children(), &[a]);
    assert_eq!(
        layout.families()[root].direct_members()[0].identity(),
        "::std::str::Utf8Error"
    );
    assert_eq!(layout.families()[b].direct_members().len(), 2);
    // A one-function singleton still has a return contract, but does not
    // become a preferred reusable child under the permitted-reuse policy.
    assert!(!layout.families()[family(&ir, &layout, "single")?].preferred_reuse());
    assert!(layout.families()[a].children().is_empty());
    Ok(())
}

#[test]
fn threshold_controls_preferred_reuse_but_no_reuse_requires_unique_ownership() -> miette::Result<()>
{
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let functions = scope(&ir, &["nested", "a", "single"])?;
    for reuse in [SourceErrorReuse::Permitted, SourceErrorReuse::Forbidden] {
        let layout = ready(
            plan_error_family_layout(&ir, &functions, policy(3, reuse)?, Broadening::Forbidden)
                .into_diagnostic()?,
        )?;
        assert_layout(&layout);
        assert!(layout.families().iter().all(|node| !node.preferred_reuse()));
        let root = family(&ir, &layout, "nested")?;
        let a = family(&ir, &layout, "a")?;
        let single = family(&ir, &layout, "single")?;
        if reuse == SourceErrorReuse::Permitted {
            assert!(
                layout
                    .families()
                    .iter()
                    .all(|node| node.children().is_empty())
            );
        } else {
            assert_eq!(layout.families()[root].children(), &[a]);
            assert_eq!(layout.families()[a].children(), &[single]);
        }
    }
    Ok(())
}

#[test]
fn nominally_equal_sources_keep_distinct_producer_sets_in_layout_nodes() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let functions = scope(&ir, &["a", "domain", "single", "two_producers"])?;
    let layout = ready(
        plan_error_family_layout(
            &ir,
            &functions,
            policy(1, SourceErrorReuse::Permitted)?,
            Broadening::Forbidden,
        )
        .into_diagnostic()?,
    )?;
    assert_layout(&layout);
    assert_eq!(layout.families().len(), 4);
    let single = family(&ir, &layout, "single")?;
    let multiple = family(&ir, &layout, "two_producers")?;
    assert_ne!(single, multiple);
    assert_eq!(layout.families()[multiple].members()[0].origins().len(), 2);
    assert_eq!(
        layout.families()[family(&ir, &layout, "a")?].children(),
        &[single]
    );
    assert!(
        layout.families()[family(&ir, &layout, "domain")?]
            .children()
            .is_empty()
    );
    Ok(())
}

#[test]
fn representation_blockers_refuse_layout_instead_of_exposing_partial_declarations()
-> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    for names in [&["a", "b"][..], &["a", "domain"][..], &["a", "unknown"][..]] {
        let functions = scope(&ir, names)?;
        let decision = plan_error_family_layout(
            &ir,
            &functions,
            policy(1, SourceErrorReuse::Forbidden)?,
            Broadening::Forbidden,
        )
        .into_diagnostic()?;
        let Decision::Refused(representation) = decision else {
            return Err(miette::miette!("expected refusal for {names:?}"));
        };
        assert!(!representation.blockers().is_empty());
    }
    let empty = ready(
        plan_error_family_layout(
            &ir,
            &scope(&ir, &["empty"])?,
            policy(1, SourceErrorReuse::Forbidden)?,
            Broadening::Forbidden,
        )
        .into_diagnostic()?,
    )?;
    assert_layout(&empty);
    assert!(empty.families().is_empty());
    assert_eq!(empty.contract_families(), &[None]);
    Ok(())
}

#[test]
fn four_source_contract_triples_expand_without_loss_duplication_or_cycles() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let functions = scope(
        &ir,
        &[
            "single",
            "parse_only",
            "utf8_only",
            "float_only",
            "a",
            "b",
            "c",
            "parse_utf8",
            "io_float",
            "parse_float",
            "nested",
            "io_parse_float",
            "io_utf8_float",
            "parse_utf8_float",
            "all",
        ],
    )?;
    for reuse in [SourceErrorReuse::Permitted, SourceErrorReuse::Forbidden] {
        for minimum in [1, 2] {
            for left in 0..functions.len() {
                for middle in left + 1..functions.len() {
                    for right in middle + 1..functions.len() {
                        let scope = [functions[left], functions[middle], functions[right]];
                        let layout = ready(
                            plan_error_family_layout(
                                &ir,
                                &scope,
                                policy(minimum, reuse)?,
                                Broadening::Permitted,
                            )
                            .into_diagnostic()?,
                        )?;
                        assert_layout(&layout);
                    }
                }
            }
        }
    }
    Ok(())
}
