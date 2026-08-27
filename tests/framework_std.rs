#![cfg(feature = "homecoming_std")]

use std::collections::HashSet;

use cordial::testing::{
    FrameworkTraitStatus, InventoryItemKind, SkipMap, StdInventoryItem, build_framework_gaps,
    build_framework_trait_report, merge_std_inventory_items,
};

fn sample_item(path: &str) -> StdInventoryItem {
    StdInventoryItem {
        path: path.to_string(),
        kind: InventoryItemKind::Struct,
        is_generic: false,
        is_unstable: false,
        alias_target: None,
    }
}

fn sample_unstable_item(path: &str) -> StdInventoryItem {
    StdInventoryItem {
        is_unstable: true,
        ..sample_item(path)
    }
}

#[test]
fn merge_std_inventories_dedupes_concrete_types() {
    cordial::init_tracing();
    let std_inv = vec![sample_item("std::collections::HashMap")];
    let core_inv = vec![
        sample_item("core::fmt::Debug"),
        sample_item("std::collections::HashMap"),
    ];
    let merged = merge_std_inventory_items(&[std_inv, core_inv]);
    assert_eq!(merged.len(), 2);
}

#[test]
fn build_framework_trait_report_classifies_complete_missing_and_skipped() {
    cordial::init_tracing();
    let source = vec![
        sample_item("std::primitive::i32"),
        sample_item("std::string::String"),
        sample_item("core::fmt::Debug"),
    ];
    let impls: HashSet<String> = ["i32", "std::string::String"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut skip = SkipMap::new();
    skip.insert("core::fmt::Debug".to_string(), "trait object".to_string());

    let report = build_framework_trait_report(
        "std",
        &source,
        "Code",
        "homecoming_core",
        &impls,
        &skip,
        false,
    );
    assert_eq!(report.complete_count, 2);
    assert_eq!(report.missing_count, 0);
    assert_eq!(report.skipped_count, 1);
    assert!(build_framework_gaps(&report).is_empty());

    let missing_source = vec![sample_item("std::vec::Vec")];
    let missing_report = build_framework_trait_report(
        "std",
        &missing_source,
        "Code",
        "homecoming_core",
        &impls,
        &SkipMap::new(),
        false,
    );
    assert_eq!(
        missing_report.entries[0].trait_status,
        FrameworkTraitStatus::Missing
    );
    assert_eq!(build_framework_gaps(&missing_report).len(), 1);
}

#[test]
fn stable_only_scope_excludes_nightly_std_types() {
    cordial::init_tracing();
    let source = vec![
        sample_item("std::string::String"),
        sample_unstable_item("std::f16"),
    ];
    let empty = HashSet::new();
    let stable_report = build_framework_trait_report(
        "std",
        &source,
        "Code",
        "homecoming_core",
        &empty,
        &SkipMap::new(),
        false,
    );
    assert_eq!(stable_report.entries.len(), 1);
    assert_eq!(stable_report.entries[0].type_path, "std::string::String");

    let nightly_report = build_framework_trait_report(
        "std",
        &source,
        "Code",
        "homecoming_core",
        &empty,
        &SkipMap::new(),
        true,
    );
    assert_eq!(nightly_report.entries.len(), 2);
}

#[test]
fn homecoming_std_plugin_is_registered() {
    cordial::init_tracing();
    use cordial::{HOMECOMING_STD_COVERAGE, Plugin, WorkspaceHub, coverage_plugins_for_hub};

    let plugins = coverage_plugins_for_hub(WorkspaceHub::Homecoming);
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].id(), HOMECOMING_STD_COVERAGE.id());
}

#[test]
fn type_has_trait_impl_matches_bare_and_qualified_paths() {
    cordial::init_tracing();
    use std::collections::HashSet;

    use cordial::testing::type_has_trait_impl;

    let impls: HashSet<String> = ["i32", "std::collections::HashMap"]
        .into_iter()
        .map(str::to_string)
        .collect();
    assert!(type_has_trait_impl(&impls, "i32"));
    assert!(type_has_trait_impl(&impls, "std::primitive::i32"));
    assert!(type_has_trait_impl(&impls, "std::collections::HashMap"));
    assert!(!type_has_trait_impl(&impls, "std::vec::Vec"));
}

#[test]
fn type_has_trait_impl_does_not_cross_match_unrelated_types_sharing_a_bare_name() {
    cordial::init_tracing();
    use std::collections::HashSet;

    use cordial::testing::type_has_trait_impl;

    let impls: HashSet<String> = HashSet::from(["core::fmt::Error".to_string()]);
    assert!(type_has_trait_impl(&impls, "core::fmt::Error"));
    assert!(!type_has_trait_impl(&impls, "std::io::Error"));
}

#[test]
fn type_has_trait_impl_matches_across_a_std_core_reexport() {
    cordial::init_tracing();
    use std::collections::HashSet;

    use cordial::testing::type_has_trait_impl;

    let impls: HashSet<String> = HashSet::from(["core::fmt::Alignment".to_string()]);
    assert!(type_has_trait_impl(&impls, "std::fmt::Alignment"));
}

#[test]
fn type_has_trait_impl_matches_representative_generic_instantiation() {
    cordial::init_tracing();
    use std::collections::HashSet;

    use cordial::testing::type_has_trait_impl;

    let impls: HashSet<String> = HashSet::from(["std::sync::mpsc::Sender<i32>".to_string()]);
    assert!(type_has_trait_impl(&impls, "std::sync::mpsc::Sender"));
}
