#![cfg(feature = "panics")]
//! Representation choices preserve exact evidence and expose policy broadening.

#[path = "support/strategy_specimens.rs"]
mod specimens;

use cordial::{
    ErrorContractBroadening as Broadening, ErrorFamilyBasis, ErrorFamilyConflict,
    ErrorFamilyContract, ErrorFamilyDisposition, ErrorFamilyMember,
    ErrorFamilyRepresentationBlocker as Blocker, ErrorFamilyRepresentationPlan,
    ErrorFamilyReusePolicy, IrView, NodeId, SourceErrorReuse, plan_error_family_representation,
};
use miette::IntoDiagnostic;
use specimens::{build, copy_specimen, discover, execute, function_facts};
use std::{fs, num::NonZeroUsize};

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

fn contract<'a>(
    ir: &dyn IrView,
    plan: &'a ErrorFamilyRepresentationPlan,
    name: &str,
) -> miette::Result<&'a ErrorFamilyContract> {
    let node = scope(ir, &[name])?[0];
    plan.contracts()
        .iter()
        .find(|contract| {
            plan.reuse().groups()[contract.exact_group()]
                .functions()
                .contains(&node)
        })
        .ok_or_else(|| miette::miette!("missing contract for {name}"))
}

fn contains(container: &[ErrorFamilyMember], subset: &[ErrorFamilyMember]) -> bool {
    subset.iter().all(|member| container.contains(member))
}

fn assert_feasible(plan: &ErrorFamilyRepresentationPlan) {
    assert!(plan.blockers().is_empty(), "{:?}", plan.blockers());
    for contract in plan.contracts() {
        let exact = &plan.reuse().groups()[contract.exact_group()];
        assert!(contains(contract.proposed_members(), exact.members()));
        assert_eq!(
            contract.added_members(),
            &contract
                .proposed_members()
                .iter()
                .filter(|member| !exact.members().contains(member))
                .cloned()
                .collect::<Vec<_>>()
        );
        if exact.disposition() == ErrorFamilyDisposition::Selected {
            assert_eq!(contract.proposed_members(), exact.members());
        }
    }
    if plan.reuse_policy().source_reuse() == SourceErrorReuse::Forbidden {
        for left in plan.contracts() {
            for right in plan.contracts() {
                let disjoint = left
                    .proposed_members()
                    .iter()
                    .all(|member| !right.proposed_members().contains(member));
                assert!(
                    disjoint
                        || contains(left.proposed_members(), right.proposed_members())
                        || contains(right.proposed_members(), left.proposed_members())
                );
            }
        }
    }
}

#[test]
fn source_reuse_preserves_crossing_contracts_and_distinct_domain_origins() -> miette::Result<()> {
    cordial::init_tracing();
    let (fixture, ir) = fixture()?;
    build(fixture.path())?;
    assert!(execute(fixture.path(), &[])?.status.success());
    assert_eq!(function_facts(&ir, "a_third")?.calls().len(), 2);
    let functions = scope(&ir, &["a", "b", "domain", "two_producers", "empty"])?;
    let before = functions
        .iter()
        .map(|id| ir.function_error_family(*id))
        .collect::<cordial::CordialResult<Vec<_>>>()
        .into_diagnostic()?;
    let source = fs::read(fixture.path().join("src/main.rs")).into_diagnostic()?;
    for broadening in [Broadening::Forbidden, Broadening::Permitted] {
        let plan = plan_error_family_representation(
            &ir,
            &functions,
            policy(1, SourceErrorReuse::Permitted)?,
            broadening,
        )
        .into_diagnostic()?;
        assert_feasible(&plan);
        for contract in plan.contracts() {
            assert_eq!(
                contract.proposed_members(),
                plan.reuse().groups()[contract.exact_group()].members()
            );
            assert!(contract.broadened_by().is_empty());
            assert!(contract.added_members().is_empty());
        }
        assert_ne!(
            contract(&ir, &plan, "a")?.proposed_members()[0].origins(),
            contract(&ir, &plan, "domain")?.proposed_members()[0].origins()
        );
    }
    assert_eq!(
        functions
            .iter()
            .map(|id| ir.function_error_family(*id))
            .collect::<cordial::CordialResult<Vec<_>>>()
            .into_diagnostic()?,
        before
    );
    assert_eq!(
        fs::read(fixture.path().join("src/main.rs")).into_diagnostic()?,
        source
    );
    Ok(())
}

#[test]
fn crossing_contract_broadens_to_an_enclosing_family_without_changing_evidence()
-> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let functions = scope(
        &ir,
        &[
            "a", "a_again", "a_third", "b", "b_again", "b_third", "nested", "single", "empty",
        ],
    )?;
    let reuse = policy(3, SourceErrorReuse::Forbidden)?;
    let plan = plan_error_family_representation(&ir, &functions, reuse, Broadening::Permitted)
        .into_diagnostic()?;
    assert_feasible(&plan);
    let a = contract(&ir, &plan, "a")?;
    let b = contract(&ir, &plan, "b")?;
    let root = contract(&ir, &plan, "nested")?;
    assert_eq!(a.proposed_members().len(), 2);
    assert_eq!(b.proposed_members(), root.proposed_members());
    assert_eq!(b.proposed_members().len(), 3);
    assert_eq!(b.added_members().len(), 1);
    assert_eq!(b.added_members()[0].identity(), "::std::num::ParseIntError");
    assert_eq!(b.added_members()[0].origins(), &scope(&ir, &["parse"])?);
    assert_eq!(b.broadened_by(), &[a.exact_group()]);
    assert_eq!(plan.reuse().groups()[b.exact_group()].members().len(), 2);
    assert!(contract(&ir, &plan, "single")?.added_members().is_empty());
    assert!(contract(&ir, &plan, "empty")?.proposed_members().is_empty());

    let refused = plan_error_family_representation(&ir, &functions, reuse, Broadening::Forbidden)
        .into_diagnostic()?;
    assert_eq!(refused.reuse(), plan.reuse());
    assert_eq!(refused.contracts(), plan.contracts());
    assert_eq!(
        refused.blockers(),
        &[Blocker::BroadeningRequired {
            group: b.exact_group()
        }]
    );
    Ok(())
}

#[test]
fn infrequent_crossings_reach_a_fixed_point_without_selected_families() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let mut functions = scope(&ir, &["a", "b", "c", "single", "empty"])?;
    let reuse = policy(3, SourceErrorReuse::Forbidden)?;
    let plan = plan_error_family_representation(&ir, &functions, reuse, Broadening::Permitted)
        .into_diagnostic()?;
    assert_feasible(&plan);
    for (name, represented, added) in [("a", 4, 2), ("b", 3, 1), ("c", 4, 2)] {
        let contract = contract(&ir, &plan, name)?;
        assert_eq!(contract.proposed_members().len(), represented, "{name}");
        assert_eq!(contract.added_members().len(), added, "{name}");
        assert!(!contract.broadened_by().is_empty());
        assert_eq!(
            plan.reuse().groups()[contract.exact_group()].disposition(),
            ErrorFamilyDisposition::BelowThreshold
        );
    }
    assert_eq!(contract(&ir, &plan, "single")?.proposed_members().len(), 1);
    functions.reverse();
    functions.push(functions[0]);
    assert_eq!(
        plan,
        plan_error_family_representation(&ir, &functions, reuse, Broadening::Permitted)
            .into_diagnostic()?
    );
    Ok(())
}

#[test]
fn distinct_producers_refuse_no_reuse_even_below_the_threshold() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let functions = scope(&ir, &["a", "domain", "two_producers"])?;
    let plan = plan_error_family_representation(
        &ir,
        &functions,
        policy(3, SourceErrorReuse::Forbidden)?,
        Broadening::Permitted,
    )
    .into_diagnostic()?;
    for left in 0..3 {
        for right in left + 1..3 {
            assert!(plan.blockers().contains(&Blocker::IncompatibleFamilies {
                left_group: left,
                right_group: right,
                conflict: ErrorFamilyConflict::DistinctProducers,
            }));
        }
    }
    assert_eq!(plan.blockers().len(), 3);
    for contract in plan.contracts() {
        assert_eq!(
            contract.proposed_members(),
            plan.reuse().groups()[contract.exact_group()].members()
        );
        assert!(contract.added_members().is_empty());
    }
    Ok(())
}

#[test]
fn consistent_nested_and_disjoint_families_need_no_broadening() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let functions = scope(
        &ir,
        &[
            "nested",
            "nested_again",
            "nested_third",
            "a",
            "a_again",
            "a_third",
            "single",
            "single_again",
            "single_third",
            "utf8_only",
            "utf8_again",
            "utf8_third",
        ],
    )?;
    let plan = plan_error_family_representation(
        &ir,
        &functions,
        policy(3, SourceErrorReuse::Forbidden)?,
        Broadening::Forbidden,
    )
    .into_diagnostic()?;
    assert_feasible(&plan);
    for contract in plan.contracts() {
        assert!(contract.added_members().is_empty());
        assert!(contract.broadened_by().is_empty());
    }
    Ok(())
}

#[test]
fn every_triple_of_four_source_contracts_preserves_nesting_and_selected_sets() -> miette::Result<()>
{
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    // All 15 nonempty subsets of four actual source identities, collected by
    // the production pipeline. Check 455 scopes at each threshold, including
    // scopes with no selected family. This checks global representation laws,
    // independently of the planner's pair traversal and expansion algorithm.
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
    for minimum in [1, 2] {
        for left in 0..functions.len() {
            for middle in left + 1..functions.len() {
                for right in middle + 1..functions.len() {
                    let scope = [functions[left], functions[middle], functions[right]];
                    let plan = plan_error_family_representation(
                        &ir,
                        &scope,
                        policy(minimum, SourceErrorReuse::Forbidden)?,
                        Broadening::Permitted,
                    )
                    .into_diagnostic()?;
                    assert_feasible(&plan);
                    let reversed = [scope[2], scope[1], scope[0], scope[0]];
                    assert_eq!(
                        plan,
                        plan_error_family_representation(
                            &ir,
                            &reversed,
                            policy(minimum, SourceErrorReuse::Forbidden)?,
                            Broadening::Permitted
                        )
                        .into_diagnostic()?
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn excluded_functions_block_acceptance_of_the_whole_scope() -> miette::Result<()> {
    cordial::init_tracing();
    let (_fixture, ir) = fixture()?;
    let mut functions = scope(&ir, &["a", "unknown", "io"])?;
    functions.push(NodeId::new(u32::MAX));
    let plan = plan_error_family_representation(
        &ir,
        &functions,
        policy(3, SourceErrorReuse::Permitted)?,
        Broadening::Permitted,
    )
    .into_diagnostic()?;
    assert_eq!(plan.blockers(), &[Blocker::IncompleteEvidence]);
    assert_eq!(plan.reuse().issues().len(), 3);
    assert_eq!(plan.contracts().len(), 1);
    assert_eq!(plan.contracts()[0].proposed_members().len(), 2);
    Ok(())
}
