//! The module hierarchy: Strahler order, branch ranking, top-heavy parents, lopsided siblings, and unary nests.

use cordial::{
    ModularityThresholds, ModuleHierarchyNode, ModuleSizeInput, build_module_hierarchy, fat_leaves,
    library_branches, lopsided_siblings, order_bands, top_heavy_parents, unary_nests,
};

#[test]
fn strahler_order_bumps_when_two_children_share_max() -> miette::Result<()> {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("left", "src/left.rs", 8),
        input("right", "src/right.rs", 8),
        input("left::a", "src/left/a.rs", 4),
        input("left::b", "src/left/b.rs", 4),
    ]);
    let crate_root = node(&nodes, "<crate>")?;
    let left = node(&nodes, "left")?;
    let right = node(&nodes, "right")?;
    assert_eq!(right.order(), 1, "single leaf child keeps order 1");
    assert_eq!(left.order(), 2, "two order-1 children bump parent to 2");
    assert_eq!(
        crate_root.order(),
        2,
        "max child order 2 with one such child"
    );
    let bands = order_bands(&nodes);
    assert!(
        bands
            .iter()
            .any(|band| band.order() == 1 && band.count() >= 3)
    );
    Ok(())
}

#[test]
fn library_branches_rank_top_heavy_parents_first() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 6),
        input("fat", "src/fat/mod.rs", 80),
        input("fat::leaf", "src/fat/leaf.rs", 8),
        input("thin", "src/thin/mod.rs", 8),
        input("thin::leaf", "src/thin/leaf.rs", 80),
    ]);
    let branches = library_branches(&nodes);
    assert_eq!(
        branches
            .iter()
            .map(|node| node.path().as_str())
            .collect::<Vec<_>>(),
        vec!["fat", "thin"]
    );
    assert!(branches[0].top_heavy() > 0.8);
    assert!(branches[1].top_heavy() < 0.2);
}

#[test]
fn fat_leaves_are_not_ranked_as_library_branches() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 6),
        input("session", "src/session.rs", 695),
        input("fat", "src/fat/mod.rs", 80),
        input("fat::leaf", "src/fat/leaf.rs", 8),
    ]);
    let branches: Vec<_> = library_branches(&nodes)
        .iter()
        .map(|node| node.path().as_str())
        .collect();
    assert_eq!(branches, vec!["fat"]);
    let leaves = fat_leaves(&nodes);
    assert_eq!(leaves[0].path(), "session");
    assert!(!leaves.iter().any(|node| node.path() == "fat"));
}

#[test]
fn top_heavy_parents_include_nested_nodes() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("pkg", "src/pkg/mod.rs", 8),
        input("pkg::loader", "src/pkg/loader.rs", 90),
        input("pkg::loader::inner", "src/pkg/loader/inner.rs", 8),
    ]);
    let parents = top_heavy_parents(&nodes);
    assert_eq!(parents[0].path(), "pkg::loader");
    assert!(parents[0].top_heavy() > 0.8);
}

#[test]
fn lopsided_siblings_rank_the_dominant_child() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("antipatterns", "src/antipatterns.rs", 3000),
        input("trenchcoat", "src/trenchcoat.rs", 300),
    ]);
    let ranked = lopsided_siblings(&nodes, 0);
    assert_eq!(ranked[0].parent(), "<crate>");
    assert_eq!(ranked[0].largest(), "antipatterns");
    assert!(ranked[0].share() > 0.8);
    assert_eq!(ranked[0].sibling_count(), 2);
    assert_eq!(ranked[0].sibling_total(), 3300);
}

#[test]
fn lopsided_siblings_ignore_stub_children() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("tracked_targets", "src/tracked_targets.rs", 275),
        input("coverage", "src/coverage.rs", 3),
    ]);
    assert!(lopsided_siblings(&nodes, 150).is_empty());
    let ranked = lopsided_siblings(&nodes, 0);
    assert_eq!(ranked[0].largest(), "tracked_targets");
    assert!(ranked[0].share() > 0.98);
}

#[test]
fn lopsided_hit_requires_seventy_five_percent_of_substantial_siblings() {
    cordial::init_tracing();
    let thresholds = ModularityThresholds::default();
    assert!(!thresholds.is_lopsided_hit(296, 462));
    assert!(thresholds.is_lopsided_hit(450, 600));
}

#[test]
fn unary_nests_rank_the_passthrough_directory() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("error", "src/error.rs", 200),
        input("error::sources", "src/error/sources/mod.rs", 14),
        input("error::sources::io", "src/error/sources/io.rs", 140),
        input("error::sources::local", "src/error/sources/local.rs", 210),
        input("error::sources::parse", "src/error/sources/parse.rs", 200),
    ]);
    let ranked = unary_nests(&nodes, 150);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].parent(), "error");
    assert_eq!(ranked[0].passthrough(), "error::sources");
    assert_eq!(ranked[0].passthrough_own(), 14);
    assert_eq!(ranked[0].grandchildren().len(), 3);
}

#[test]
fn unary_nests_ignore_a_unary_leaf_child() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("chain_layer", "src/chain_layer/mod.rs", 194),
        input("chain_layer::preds", "src/chain_layer/preds.rs", 352),
    ]);
    assert!(unary_nests(&nodes, 150).is_empty());
}

#[test]
fn unary_nests_ignore_crate_root_unary_package() {
    cordial::init_tracing();
    let nodes = build_module_hierarchy(&[
        input("<crate>", "src/lib.rs", 10),
        input("only", "src/only/mod.rs", 8),
        input("only::a", "src/only/a.rs", 80),
        input("only::b", "src/only/b.rs", 80),
    ]);
    assert!(unary_nests(&nodes, 150).is_empty());
}

#[test]
fn collapse_hit_requires_hierarchy_min_subtree() {
    cordial::init_tracing();
    let thresholds = ModularityThresholds::default();
    assert!(!thresholds.is_collapse_hit(149));
    assert!(thresholds.is_collapse_hit(150));
}

fn input(path: &str, file: &str, lines: u32) -> ModuleSizeInput {
    ModuleSizeInput::new(path.to_string(), file.to_string(), lines)
}

fn node<'a>(
    nodes: &'a [ModuleHierarchyNode],
    path: &str,
) -> miette::Result<&'a ModuleHierarchyNode> {
    nodes
        .iter()
        .find(|node| node.path() == path)
        .ok_or_else(|| miette::miette!("missing {path}"))
}
