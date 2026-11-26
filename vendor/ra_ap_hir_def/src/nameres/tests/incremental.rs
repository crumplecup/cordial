use base_db::{
    CrateDisplayName, CrateGraphBuilder, CrateName, CrateOrigin, CrateWorkspaceData,
    DependencyBuilder, Env, SourceDatabase, all_crates,
};
use expect_test::{Expect, expect};
use intern::Symbol;
use span::Edition;
use test_fixture::WithFixture;
use triomphe::Arc;

use crate::{
    file_item_tree,
    nameres::{crate_def_map, tests::TestDB},
};

fn check_def_map_is_not_recomputed(
    #[rust_analyzer::rust_fixture] ra_fixture_initial: &str,
    #[rust_analyzer::rust_fixture] ra_fixture_change: &str,
    expecta: Expect,
    expectb: Expect,
) {
    let (mut db, pos) = TestDB::with_position(ra_fixture_initial);
    let krate = db.fetch_test_crate();
    execute_assert_events(
        &db,
        || {
            crate_def_map(&db, krate);
        },
        &[],
        expecta,
    );
    db.set_file_text(pos.file_id.file_id(&db), ra_fixture_change);

    execute_assert_events(
        &db,
        || {
            crate_def_map(&db, krate);
        },
        &[("crate_local_def_map", 0)],
        expectb,
    );
}

#[test]
fn crate_metadata_changes_should_not_invalidate_unrelated_def_maps() {
    let (mut db, files) = TestDB::with_many_files(
        r#"
//- /a.rs crate:a
pub fn foo() {}

//- /b.rs crate:b
pub struct Bar;

//- /c.rs crate:c deps:b
pub const BAZ: u32 = 0;
    "#,
    );

    for &krate in all_crates(&db).iter() {
        crate_def_map(&db, krate);
    }

    let all_crates_before = all_crates(&db);

    {
        // Add dependencies: c -> b, b -> a.
        let mut new_crate_graph = CrateGraphBuilder::default();

        let mut add_crate = |crate_name, root_file_idx: usize| {
            new_crate_graph.add_crate_root(
                files[root_file_idx].file_id(&db),
                Edition::CURRENT,
                Some(CrateDisplayName::from_canonical_name(crate_name)),
                None,
                Default::default(),
                None,
                Env::default(),
                CrateOrigin::Local { repo: None, name: Some(Symbol::intern(crate_name)) },
                Vec::new(),
                false,
                Arc::new(
                    // FIXME: This is less than ideal
                    TryFrom::try_from(
                        &*std::env::current_dir().unwrap().as_path().to_string_lossy(),
                    )
                    .unwrap(),
                ),
                Arc::new(CrateWorkspaceData { target: Err("".into()), toolchain: None }),
            )
        };
        let a = add_crate("a", 0);
        let b = add_crate("b", 1);
        let c = add_crate("c", 2);
        new_crate_graph
            .add_dep(c, DependencyBuilder::new(CrateName::new("b").unwrap(), b))
            .unwrap();
        new_crate_graph
            .add_dep(b, DependencyBuilder::new(CrateName::new("a").unwrap(), a))
            .unwrap();
        new_crate_graph.set_in_db(&mut db);
    }

    let all_crates_after = all_crates(&db);
    assert!(
        std::sync::Arc::ptr_eq(&all_crates_before, &all_crates_after),
        "the all_crates list should not have been invalidated"
    );
    execute_assert_events(
        &db,
        || {
            for &krate in all_crates(&db).iter() {
                crate_def_map(&db, krate);
            }
        },
        // `b` gets invalidated due to its new dependency edge to `a`.
        // `c` is not: it reads the modules of `b` through their cells, and the new edge changes `b`'s extern
        // prelude, which is not among them. Upstream's map was one value that `c` read, so it ran again.
        &[("crate_local_def_map", 1)],
        expect![[r#"
            [
                "crate_local_def_map",
                "file_item_tree_query",
                "external_def_map",
            ]
        "#]],
    );
}

#[test]
fn typing_inside_a_function_should_not_invalidate_def_map() {
    check_def_map_is_not_recomputed(
        r"
//- /lib.rs
mod foo;$0

use crate::foo::bar::Baz;

enum E { A, B }
use E::*;

fn foo() -> i32 {
    1 + 1
}

#[cfg(never)]
fn no() {}
//- /foo/mod.rs
pub mod bar;

//- /foo/bar.rs
pub struct Baz;
",
        r"
mod foo;

use crate::foo::bar::Baz;

enum E { A, B }
use E::*;

fn foo() -> i32 { 92 }

#[cfg(never)]
fn no() {}
",
        expect![[r#"
            [
                "external_def_map",
                "crate_local_def_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "cfg_atom_enabled",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "EnumVariants::of_",
            ]
        "#]],
        expect![[r#"
            [
                "EditionedFileId::parse_",
                "HirFileId::ast_id_map_",
                "file_item_tree_query",
                "real_span_map",
                "EnumVariants::of_",
            ]
        "#]],
    );
}

#[test]
fn typing_inside_a_macro_should_not_invalidate_def_map() {
    check_def_map_is_not_recomputed(
        r"
//- /lib.rs
macro_rules! m {
    ($ident:ident) => {
        fn f() {
            $ident + $ident;
        };
    }
}
mod foo;

//- /foo/mod.rs
pub mod bar;

//- /foo/bar.rs
$0
m!(X);

pub struct S {}
",
        r"
m!(Y);

pub struct S {}
",
        expect![[r#"
            [
                "external_def_map",
                "crate_local_def_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "DeclarativeMacroKey < 'db >::expander_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "MacroId::definition_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "MacroCallId::parse_macro_expansion_",
                "expand_pure_macro_of",
                "MacroCallId::macro_arg_",
            ]
        "#]],
        expect![[r#"
            [
                "EditionedFileId::parse_",
                "HirFileId::ast_id_map_",
                "file_item_tree_query",
                "real_span_map",
                "MacroCallId::macro_arg_",
                "expand_pure_macro_of",
                "MacroCallId::parse_macro_expansion_",
                "HirFileId::ast_id_map_",
                "file_item_tree_query",
            ]
        "#]],
    );
}

#[test]
fn typing_inside_an_attribute_should_not_invalidate_def_map() {
    check_def_map_is_not_recomputed(
        r"
//- proc_macros: identity
//- /lib.rs
mod foo;

//- /foo/mod.rs
pub mod bar;

//- /foo/bar.rs
$0
#[proc_macros::identity]
fn f() {}
",
        r"
#[proc_macros::identity]
fn f() { foo }
",
        expect![[r#"
            [
                "external_def_map",
                "crate_local_def_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "external_def_map",
                "crate_local_def_map",
                "ProcMacros::get_for_crate_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "crate_module_cell",
                "crate_module_cells",
                "MacroId::definition_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "MacroCallId::parse_macro_expansion_",
                "expand_proc_macro_of",
                "MacroCallId::macro_arg_",
                "proc_macro_span",
            ]
        "#]],
        expect![[r#"
            [
                "EditionedFileId::parse_",
                "HirFileId::ast_id_map_",
                "file_item_tree_query",
                "real_span_map",
                "MacroCallId::macro_arg_",
                "expand_proc_macro_of",
                "MacroCallId::parse_macro_expansion_",
                "HirFileId::ast_id_map_",
                "file_item_tree_query",
            ]
        "#]],
    );
}

// Would be nice if this was the case, but as attribute inputs are stored in the item tree, this is
// not currently the case.
// #[test]
// fn typing_inside_an_attribute_arg_should_not_invalidate_def_map() {
//     check_def_map_is_not_recomputed(
//         r"
// //- proc_macros: identity
// //- /lib.rs
// mod foo;

// //- /foo/mod.rs
// pub mod bar;

// //- /foo/bar.rs
// $0
// #[proc_macros::identity]
// fn f() {}
// ",
//         r"
// #[proc_macros::identity(foo)]
// fn f() {}
// ",
//     );
// }

#[test]
fn typing_inside_macro_heavy_file_should_not_invalidate_def_map() {
    check_def_map_is_not_recomputed(
        r"
//- proc_macros: identity, derive_identity
//- /lib.rs
macro_rules! m {
    ($ident:ident) => {
        fn fm() {
            $ident + $ident;
        };
    }
}
mod foo;

//- /foo/mod.rs
pub mod bar;

//- /foo/bar.rs
$0
fn f() {}

m!(X);
macro_rules! m2 {
    ($ident:ident) => {
        fn f2() {
            $ident + $ident;
        };
    }
}
m2!(X);

#[proc_macros::identity]
#[derive(proc_macros::DeriveIdentity)]
pub struct S {}
",
        r"
fn f() {0}

m!(X);
macro_rules! m2 {
    ($ident:ident) => {
        fn f2() {
            $ident + $ident;
        };
    }
}
m2!(X);

#[proc_macros::identity]
#[derive(proc_macros::DeriveIdentity)]
pub struct S {}
",
        expect![[r#"
            [
                "external_def_map",
                "crate_local_def_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "external_def_map",
                "crate_local_def_map",
                "ProcMacros::get_for_crate_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "DeclarativeMacroKey < 'db >::expander_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "MacroId::definition_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "MacroCallId::parse_macro_expansion_",
                "expand_pure_macro_of",
                "MacroCallId::macro_arg_",
                "DeclarativeMacroKey < 'db >::expander_",
                "MacroId::definition_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "MacroCallId::parse_macro_expansion_",
                "expand_pure_macro_of",
                "MacroCallId::macro_arg_",
                "crate_module_cell",
                "crate_module_cells",
                "MacroId::definition_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "MacroCallId::parse_macro_expansion_",
                "expand_proc_macro_of",
                "MacroCallId::macro_arg_",
                "proc_macro_span",
            ]
        "#]],
        expect![[r#"
            [
                "EditionedFileId::parse_",
                "HirFileId::ast_id_map_",
                "file_item_tree_query",
                "real_span_map",
                "MacroCallId::macro_arg_",
                "DeclarativeMacroKey < 'db >::expander_",
                "MacroCallId::macro_arg_",
                "MacroCallId::macro_arg_",
            ]
        "#]],
    );
}

// Would be nice if this was the case, but as attribute inputs are stored in the item tree, this is
// not currently the case.
// #[test]
// fn typing_inside_a_derive_should_not_invalidate_def_map() {
//     check_def_map_is_not_recomputed(
//         r"
// //- proc_macros: derive_identity
// //- minicore:derive
// //- /lib.rs
// mod foo;

// //- /foo/mod.rs
// pub mod bar;

// //- /foo/bar.rs
// $0
// #[derive(proc_macros::DeriveIdentity)]
// #[allow()]
// struct S;
// ",
//         r"
// #[derive(proc_macros::DeriveIdentity)]
// #[allow(dead_code)]
// struct S;
// ",
//     );
// }

#[test]
fn typing_inside_a_function_should_not_invalidate_item_expansions() {
    let (mut db, pos) = TestDB::with_position(
        r#"
//- /lib.rs
macro_rules! m {
    ($ident:ident) => {
        fn $ident() { };
    }
}
mod foo;

//- /foo/mod.rs
pub mod bar;

//- /foo/bar.rs
m!(X);
fn quux() { 1$0 }
m!(Y);
m!(Z);
"#,
    );
    let krate = db.test_crate();
    execute_assert_events(
        &db,
        || {
            let crate_def_map = crate_def_map(&db, krate);
            let module_data = crate_def_map.module_data(
                &db,
                crate_def_map.modules_for_file(&db, pos.file_id.file_id(&db)).next().unwrap(),
            );
            assert_eq!(module_data.scope.resolutions().count(), 4);
        },
        &[("file_item_tree_query", 6), ("parse_macro_expansion", 3)],
        expect![[r#"
            [
                "external_def_map",
                "crate_local_def_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "DeclarativeMacroKey < 'db >::expander_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
                "MacroId::definition_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "MacroCallId::parse_macro_expansion_",
                "expand_pure_macro_of",
                "MacroCallId::macro_arg_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "MacroCallId::parse_macro_expansion_",
                "expand_pure_macro_of",
                "MacroCallId::macro_arg_",
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "MacroCallId::parse_macro_expansion_",
                "expand_pure_macro_of",
                "MacroCallId::macro_arg_",
                "crate_modules_in_file",
                "crate_modules_cell",
                "crate_module_cell",
                "crate_module_cells",
            ]
        "#]],
    );

    let new_text = r#"
m!(X);
fn quux() { 92 }
m!(Y);
m!(Z);
"#;
    db.set_file_text(pos.file_id.file_id(&db), new_text);

    execute_assert_events(
        &db,
        || {
            let crate_def_map = crate_def_map(&db, krate);
            let module_data = crate_def_map.module_data(
                &db,
                crate_def_map.modules_for_file(&db, pos.file_id.file_id(&db)).next().unwrap(),
            );
            assert_eq!(module_data.scope.resolutions().count(), 4);
        },
        &[("file_item_tree_query", 1), ("parse_macro_expansion", 0)],
        expect![[r#"
            [
                "EditionedFileId::parse_",
                "HirFileId::ast_id_map_",
                "file_item_tree_query",
                "real_span_map",
                "MacroCallId::macro_arg_",
                "MacroCallId::macro_arg_",
                "MacroCallId::macro_arg_",
            ]
        "#]],
    );
}

#[test]
fn item_tree_prevents_reparsing() {
    let (mut db, pos) = TestDB::with_position(
        r#"
pub struct S;
pub union U {}
pub enum E {
    Variant,
}
pub fn f(_: S) { $0 }
pub trait Tr {}
impl Tr for () {}
pub const C: u8 = 0;
pub static ST: u8 = 0;
pub type Ty = ();
"#,
    );

    execute_assert_events(
        &db,
        || {
            file_item_tree(&db, pos.file_id.into(), db.test_crate());
        },
        &[("file_item_tree_query", 1), ("parse", 1)],
        expect![[r#"
            [
                "file_item_tree_query",
                "HirFileId::ast_id_map_",
                "EditionedFileId::parse_",
                "real_span_map",
            ]
        "#]],
    );

    let file_id = pos.file_id.file_id(&db);
    let file_text = db.file_text(file_id).text(&db);
    db.set_file_text(file_id, &format!("{file_text}\n"));

    execute_assert_events(
        &db,
        || {
            file_item_tree(&db, pos.file_id.into(), db.test_crate());
        },
        &[("file_item_tree_query", 1), ("parse", 1)],
        expect![[r#"
            [
                "EditionedFileId::parse_",
                "HirFileId::ast_id_map_",
                "file_item_tree_query",
                "real_span_map",
            ]
        "#]],
    );
}

fn execute_assert_events(
    db: &TestDB,
    f: impl FnOnce(),
    required: &[(&str, usize)],
    expect: Expect,
) {
    let events = db.log_executed(f);
    for (event, count) in required {
        let n = events.iter().filter(|it| it.contains(event)).count();
        assert_eq!(n, *count, "Expected {event} to be executed {count} times, but only got {n}");
    }
    expect.assert_debug_eq(&events);
}

/// How many invocations of attribute macros the modules of the crate's map hold.
fn attribute_macro_invocations(db: &TestDB, krate: base_db::Crate) -> usize {
    crate_def_map(db, krate).modules(db).map(|(_, module)| module.scope.attr_macro_invocs().count()).sum()
}

/// The crates of the fixture that are not proc-macro crates it brings in.
fn local_crates(db: &TestDB) -> Vec<base_db::Crate> {
    all_crates(db)
        .iter()
        .copied()
        .filter(|krate| matches!(krate.data(db).origin, CrateOrigin::Local { .. }))
        .collect()
}

/// An attribute macro that a `cfg_attr` applies is in the map while the cfg applies it, and is not when the cfg is flipped
/// so that it does not: in each crate that has the file, as if the map had been made for the flipped cfg from nothing.
#[test]
fn flipping_a_cfg_that_turns_off_an_attribute_macro_takes_its_invocation_out_of_the_map_of_every_crate_that_has_the_file() {
    use salsa::Setter as _;
    let fixture = r#"
//- proc_macros: identity
//- /a.rs crate:a
mod m;
//- /b.rs crate:b
#[path = "m.rs"]
mod m;
//- /m.rs
#[cfg_attr(not(flag), proc_macros::identity)]
pub fn f() {}
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    // The crates that have the file, which the macro applies in while `flag` is not set.
    let crates: Vec<_> =
        local_crates(&db).into_iter().filter(|krate| attribute_macro_invocations(&db, *krate) == 1).collect();
    assert_eq!(crates.len(), 2, "the file is in two crates: {crates:?}");
    for &krate in &crates {
        let mut flipped = krate.cfg_options(&db).clone();
        flipped.insert_atom(Symbol::intern("flag"));
        krate.set_cfg_options(&mut db).to(flipped);
    }
    for &krate in &crates {
        assert_eq!(attribute_macro_invocations(&db, krate), 0, "and does not when it is, in {krate:?}");
    }
}

/// A file that two crates have is lowered once, and each crate has its own cfg: flipping the cfg of one of them is not
/// flipping the other's, and the other still has the attribute macro its own cfg applies.
#[test]
fn flipping_the_cfg_of_one_crate_that_has_a_file_leaves_the_attribute_macro_the_other_applies() {
    use salsa::Setter as _;
    let fixture = r#"
//- proc_macros: identity
//- /a.rs crate:a
mod m;
//- /b.rs crate:b
#[path = "m.rs"]
mod m;
//- /m.rs
#[cfg_attr(not(flag), proc_macros::identity)]
pub fn f() {}
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let crates: Vec<_> =
        local_crates(&db).into_iter().filter(|krate| attribute_macro_invocations(&db, *krate) == 1).collect();
    assert_eq!(crates.len(), 2, "the file is in two crates: {crates:?}");
    let (flipped_crate, other) = (crates[0], crates[1]);
    let mut flipped = flipped_crate.cfg_options(&db).clone();
    flipped.insert_atom(Symbol::intern("flag"));
    flipped_crate.set_cfg_options(&mut db).to(flipped);
    assert_eq!(attribute_macro_invocations(&db, flipped_crate), 0, "the macro is off in the crate that was flipped");
    assert_eq!(attribute_macro_invocations(&db, other), 1, "and still applies in the one that was not");
}

/// What the events of one target said, by message's target and the names of their fields.
#[derive(Clone, Default)]
struct EventsSaid(std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<&'static str>)>>>);

impl tracing::Subscriber for EventsSaid {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let fields = event.metadata().fields().iter().map(|field| field.name()).collect();
        self.0.lock().unwrap().push((event.metadata().target().to_owned(), fields));
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

/// The call of an attribute macro that a cfg applied, asked to be expanded after the cfg is flipped so that it does not
/// apply it: it cannot be, and the call says which crate made it, under which cfg and for which attribute, before it
/// fails, so that a log of a run that fails there says what the call was.
#[test]
fn an_attribute_macro_call_that_the_cfg_no_longer_has_says_what_it_is_before_it_fails() {
    use salsa::Setter as _;
    let fixture = r#"
//- proc_macros: identity
//- /main.rs crate:main
#[cfg_attr(not(flag), proc_macros::identity)]
pub fn f() {}
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let krate = db.fetch_test_crate();
    let call = crate_def_map(&db, krate)
        .modules(&db)
        .flat_map(|(_, module)| module.scope.attr_macro_invocs().map(|(_, call)| call).collect::<Vec<_>>())
        .next()
        .expect("the macro applies while `flag` is not set");
    let mut flipped = krate.cfg_options(&db).clone();
    flipped.insert_atom(Symbol::intern("flag"));
    krate.set_cfg_options(&mut db).to(flipped);

    let said = EventsSaid::default();
    let result = tracing::subscriber::with_default(said.clone(), || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let kind = call.loc(&db).kind.clone();
            let _ = call.macro_arg_considering_derives(&db, &kind);
        }))
    });
    assert!(result.is_err(), "a call for an attribute the cfg no longer has cannot be expanded");
    let said = said.0.lock().unwrap().clone();
    let event = said
        .iter()
        .find(|(target, _)| target == "cordial::attrs")
        .unwrap_or_else(|| panic!("the call said what it is before it failed: {said:?}"));
    for field in ["call", "krate", "krate_name", "attr_index", "cfg_atoms_that_are_not_the_target", "in_file", "item"] {
        assert!(event.1.contains(&field), "the event says `{field}`: {event:?}");
    }
}

/// A map made again after a flip says which attribute macro calls the modules it took from the map before held, so that a
/// call of an attribute macro that the cfg turned off, still in the map, is told to be one that a reused module kept.
#[test]
fn a_map_made_again_says_which_attribute_macro_calls_its_reused_modules_held() {
    use salsa::Setter as _;
    let fixture = r#"
//- proc_macros: identity
//- /main.rs crate:main
mod a;
mod b;
//- /a.rs
#[cfg_attr(not(flag), proc_macros::identity)]
pub fn f() {}
//- /b.rs
pub fn untouched() {}
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let krate = db.fetch_test_crate();
    let _ = crate_def_map(&db, krate);
    let mut flipped = krate.cfg_options(&db).clone();
    flipped.insert_atom(Symbol::intern("flag"));
    krate.set_cfg_options(&mut db).to(flipped);
    let said = EventsSaid::default();
    tracing::subscriber::with_default(said.clone(), || {
        let _ = crate_def_map(&db, krate);
    });
    let said = said.0.lock().unwrap().clone();
    let built = said
        .iter()
        .find(|(target, fields)| target == "cordial::def_map" && fields.contains(&"taken_attr_macro_calls"))
        .unwrap_or_else(|| panic!("the map says what its reused modules held: {said:?}"));
    assert!(built.1.contains(&"taken_from_original"), "{built:?}");
}

/// The definition a crate's map gives a name in the module the crate has at its root, if it gives one in the value
/// namespace.
fn value_named(db: &TestDB, krate: base_db::Crate, name: &str) -> Option<crate::ModuleDefId> {
    let name = hir_expand::name::Name::new_symbol_root(Symbol::intern(name));
    crate_def_map(db, krate)
        .modules(db)
        .filter(|(_, module)| module.parent.is_none())
        .find_map(|(_, module)| module.scope.get(&name).take_values())
}

/// A crate that re-exports a function of another, and the function an attribute macro made while a cfg applied it: both
/// crates flipped so that the macro does not apply, the dependent's name is the function the other crate gives it, and
/// not the one the macro made.
#[test]
fn a_crate_that_reexports_a_function_has_the_function_its_dependency_has_after_a_flip_turns_the_macro_off() {
    use salsa::Setter as _;
    let fixture = r#"
//- proc_macros: identity
//- /a.rs crate:a
#[cfg_attr(not(flag), proc_macros::identity)]
pub fn w() {}
//- /b.rs crate:b deps:a
pub use a::w;
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let crates: Vec<_> = local_crates(&db);
    let (a, b) = crates
        .iter()
        .copied()
        .find_map(|a| crates.iter().copied().find(|b| *b != a && value_named(&db, *b, "w").is_some() && value_named(&db, a, "w").is_some()).map(|b| (a, b)))
        .expect("both crates have a `w`");
    // The crate with the attribute is the one whose map declares the call.
    let (a, b) = if attribute_macro_invocations(&db, a) == 1 { (a, b) } else { (b, a) };
    assert_eq!(value_named(&db, b, "w"), value_named(&db, a, "w"), "before the flip the dependent has the function its dependency has");
    for krate in [a, b] {
        let mut flipped = krate.cfg_options(&db).clone();
        flipped.insert_atom(Symbol::intern("flag"));
        krate.set_cfg_options(&mut db).to(flipped);
    }
    assert_eq!(attribute_macro_invocations(&db, a), 0, "the macro is off in the crate that has it");
    assert_eq!(
        value_named(&db, b, "w"),
        value_named(&db, a, "w"),
        "after the flip the dependent has the function its dependency has, and not the one the macro made"
    );
}

/// The definition a module that is not the crate's root gives a name in the value namespace.
fn value_named_below_the_root(db: &TestDB, krate: base_db::Crate, name: &str) -> Option<crate::ModuleDefId> {
    let name = hir_expand::name::Name::new_symbol_root(Symbol::intern(name));
    crate_def_map(db, krate)
        .modules(db)
        .filter(|(_, module)| module.parent.is_some())
        .find_map(|(_, module)| module.scope.get(&name).take_values())
}

/// A child module of the dependent crate that imports what the crate's root re-exports from its dependency: both crates
/// flipped so that the attribute macro of the function does not apply, the child has the function the dependency has, and
/// not the one the macro made.
#[test]
fn a_module_that_imports_what_its_crate_reexports_has_the_function_its_dependency_has_after_a_flip() {
    use salsa::Setter as _;
    let fixture = r#"
//- proc_macros: identity
//- /a.rs crate:a
#[cfg_attr(not(flag), proc_macros::identity)]
pub fn w() {}
//- /b.rs crate:b deps:a
pub use a::w;
mod m;
//- /m.rs
use crate::w;
pub fn call() { w(); }
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let crates: Vec<_> = local_crates(&db);
    let a = *crates.iter().find(|krate| attribute_macro_invocations(&db, **krate) == 1).expect("crate a has the call");
    let b = *crates.iter().find(|krate| value_named_below_the_root(&db, **krate, "w").is_some()).expect("crate b has `m`");
    assert_eq!(value_named_below_the_root(&db, b, "w"), value_named(&db, a, "w"), "before the flip the child has the function `a` has");
    for krate in [a, b] {
        let mut flipped = krate.cfg_options(&db).clone();
        flipped.insert_atom(Symbol::intern("flag"));
        krate.set_cfg_options(&mut db).to(flipped);
    }
    assert_eq!(attribute_macro_invocations(&db, a), 0, "the macro is off in the crate that had it");
    assert_eq!(
        value_named_below_the_root(&db, b, "w"),
        value_named(&db, a, "w"),
        "after the flip the child has the function `a` has, and not the one the macro made"
    );
}

/// What a query that asks whether a function is declared reads of the module: a stand-in for what the tool asks of
/// every function it evaluates.
#[salsa::tracked]
fn asks_whether_declared(db: &dyn SourceDatabase, function: crate::FunctionId) -> bool {
    crate::declaration::function_declaration(db, function).is_declared()
}

/// A name added to a module (a function a cfg now declares) does not make what asks whether another function of the
/// module is declared run again: the answer is the same, and it is the answer that is read, not the module's scope.
#[test]
fn what_asks_whether_a_function_is_declared_does_not_run_again_when_a_name_is_added_to_its_module() {
    use salsa::Setter as _;
    let fixture = r#"
//- /a.rs crate:a
pub fn stays() {}

#[cfg(flag)]
pub fn appears() {}
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let krate = local_crates(&db)[0];
    let Some(crate::ModuleDefId::FunctionId(stays)) = value_named(&db, krate, "stays") else {
        panic!("the fixture has the function");
    };
    assert!(asks_whether_declared(&db, stays), "the function is declared");
    assert!(value_named(&db, krate, "appears").is_none(), "not vacuous: the module does not have the other function yet");

    let mut flipped = krate.cfg_options(&db).clone();
    flipped.insert_atom(Symbol::intern("flag"));
    krate.set_cfg_options(&mut db).to(flipped);
    assert!(value_named(&db, krate, "appears").is_some(), "not vacuous: the flip added a name to the module");

    let executed = db.log_executed(|| {
        assert!(asks_whether_declared(&db, stays), "and the function is still declared");
    });
    let ran = |name: &str| executed.iter().filter(|event| event.contains(name)).count();
    assert!(ran("module_declares") >= 1, "the module's declarations are read again, for they changed: {executed:?}");
    assert_eq!(ran("function_in_scope"), 1, "and the answer for the function is made again, to be compared: {executed:?}");
    assert_eq!(ran("asks_whether_declared"), 0, "what read the answer is not run again, for it is the same: {executed:?}");
}

/// What a function that calls into another crate asks of the definition maps: the path `inner::helper`, from the root of
/// the crate that calls it.
#[salsa::tracked]
fn resolves_a_path_into_another_crate(db: &dyn SourceDatabase, krate: base_db::Crate) -> bool {
    use hir_expand::{mod_path::{ModPath, PathKind}, name::Name};

    let map = crate_def_map(db, krate);
    let path = ModPath::from_segments(
        PathKind::Plain,
        [Name::new_symbol_root(Symbol::intern("inner")), Name::new_symbol_root(Symbol::intern("helper"))],
    );
    let found = map.resolve_path_fp_with_macro(
        crate::nameres::local_def_map(db, krate),
        db,
        crate::nameres::path_resolution::ResolveMode::Other,
        map.root_module_id(),
        &path,
        crate::nameres::BuiltinShadowMode::Module,
        None,
    );
    found.resolved_def.take_values().is_some()
}

/// Resolving a path into another crate does not run again when a name is added to the root of either crate: what it
/// reads of each is the name it asks for.
#[test]
fn resolving_a_path_into_another_crate_does_not_run_again_when_a_name_is_added_to_a_root() {
    use salsa::Setter as _;
    let fixture = r#"
//- /inner.rs crate:inner
pub fn helper() {}

#[cfg(flag)]
pub fn appears_in_inner() {}
//- /gated.rs crate:gated deps:inner
pub fn ordinary() { inner::helper(); }

#[cfg(flag)]
pub fn appears_in_gated() {}
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let crates = local_crates(&db);
    let gated = *crates.iter().find(|krate| value_named(&db, **krate, "ordinary").is_some()).expect("crate gated");
    assert!(resolves_a_path_into_another_crate(&db, gated), "not vacuous: the path resolves");

    for krate in &crates {
        let mut flipped = krate.cfg_options(&db).clone();
        flipped.insert_atom(Symbol::intern("flag"));
        krate.set_cfg_options(&mut db).to(flipped);
    }
    assert!(value_named(&db, gated, "appears_in_gated").is_some(), "not vacuous: the flip added a name to the root");
    let executed = db.log_executed(|| {
        assert!(resolves_a_path_into_another_crate(&db, gated), "and the path still resolves");
    });
    assert!(
        !executed.iter().any(|event| event.contains("resolves_a_path_into_another_crate")),
        "what resolved the path was run again, for a name it did not ask for: {executed:?}"
    );
}

/// The function a syntax node is, found from where it is written: what the semantic layer asks to map a node to its
/// definition.
#[salsa::tracked]
fn asks_for_the_function_written_here(db: &dyn SourceDatabase, function: crate::FunctionId) -> bool {
    use crate::{AstIdLoc as _, ItemContainerId};
    let loc = function.loc(db);
    let ItemContainerId::ModuleId(module) = loc.container else { return false };
    crate::declaration::declared_function(db, module, loc.ast_id()) == Some(function)
}

/// The function that a syntax node is, found from where it is written, is the function the module's scope declares, and
/// there is none when a cfg switches the function off.
#[test]
fn the_function_a_node_is_found_from_where_it_is_written_is_the_one_the_scope_declares() {
    use salsa::Setter as _;
    let fixture = r#"
//- /a.rs crate:a
pub fn stays() {}

#[cfg(flag)]
pub fn appears() {}
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let krate = local_crates(&db)[0];
    let Some(crate::ModuleDefId::FunctionId(stays)) = value_named(&db, krate, "stays") else {
        panic!("the fixture has the function");
    };
    assert!(asks_for_the_function_written_here(&db, stays), "the scope's function is the one found from its node");

    let ordinary = krate.cfg_options(&db).clone();
    let mut flipped = ordinary.clone();
    flipped.insert_atom(Symbol::intern("flag"));
    krate.set_cfg_options(&mut db).to(flipped);
    let Some(crate::ModuleDefId::FunctionId(appears)) = value_named(&db, krate, "appears") else {
        panic!("the flip declares the function");
    };
    assert!(asks_for_the_function_written_here(&db, appears), "and the function a cfg declares, while it does");
    krate.set_cfg_options(&mut db).to(ordinary);
    assert!(value_named(&db, krate, "appears").is_none(), "not vacuous: the cfg is back and the scope has no such function");
    assert!(
        !asks_for_the_function_written_here(&db, appears),
        "the id of a function that a cfg no longer declares names no function"
    );
    assert!(asks_for_the_function_written_here(&db, stays), "and the other is as it was");
}

/// Finding the function a node is from where it is written does not run again when a name is added to its module.
#[test]
fn finding_the_function_a_node_is_does_not_run_again_when_a_name_is_added_to_its_module() {
    use salsa::Setter as _;
    let fixture = r#"
//- /a.rs crate:a
pub fn stays() {}

#[cfg(flag)]
pub fn appears() {}
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let krate = local_crates(&db)[0];
    let Some(crate::ModuleDefId::FunctionId(stays)) = value_named(&db, krate, "stays") else {
        panic!("the fixture has the function");
    };
    assert!(asks_for_the_function_written_here(&db, stays));
    let mut flipped = krate.cfg_options(&db).clone();
    flipped.insert_atom(Symbol::intern("flag"));
    krate.set_cfg_options(&mut db).to(flipped);
    assert!(value_named(&db, krate, "appears").is_some(), "not vacuous: the flip added a name to the module");
    let executed = db.log_executed(|| {
        assert!(asks_for_the_function_written_here(&db, stays));
    });
    assert!(
        !executed.iter().any(|event| event.contains("asks_for_the_function_written_here")),
        "what asked for the function was run again, for a name it did not ask for: {executed:?}"
    );
}

/// A process restored from a saved state has the cells of every crate's map and not the maps they were made from. The
/// first time a map is made in it, nothing says what changed from the map before, so every key of the crate has: a flip
/// that adds a name to a root makes every reader of any name of it run again, `helper` included.
#[test]
fn resolving_a_path_into_another_crate_does_not_run_again_when_a_name_is_added_to_a_root_after_a_restore() {
    use salsa::Setter as _;
    let fixture = r#"
//- /inner.rs crate:inner
pub fn helper() {}

#[cfg(flag)]
pub fn appears_in_inner() {}
//- /gated.rs crate:gated deps:inner
pub fn ordinary() { inner::helper(); }
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let crates = local_crates(&db);
    let gated = *crates.iter().find(|krate| value_named(&db, **krate, "ordinary").is_some()).expect("crate gated");
    assert!(resolves_a_path_into_another_crate(&db, gated), "not vacuous: the path resolves");

    // The state is restored: the cells are there and nothing remembers the maps, and what they hold is taken before anything
    // is written.
    super::super::previous::forget_all(&db);
    crate::nameres::seed_previous_maps_from_cells(&mut db);
    for krate in &crates {
        let mut flipped = krate.cfg_options(&db).clone();
        flipped.insert_atom(Symbol::intern("flag"));
        krate.set_cfg_options(&mut db).to(flipped);
    }
    let inner = *crates.iter().find(|krate| **krate != gated).expect("crate inner");
    assert!(value_named(&db, inner, "appears_in_inner").is_some(), "not vacuous: the flip added a name to the root");
    let executed = db.log_executed(|| {
        assert!(resolves_a_path_into_another_crate(&db, gated), "and the path still resolves");
    });
    assert!(
        !executed.iter().any(|event| event.contains("resolves_a_path_into_another_crate")),
        "what resolved the path was run again, for a name it did not ask for: {executed:?}"
    );
}

/// Where a module comes from, asked of the root of a crate.
#[salsa::tracked]
fn asks_where_the_root_comes_from(db: &dyn SourceDatabase, krate: base_db::Crate) -> bool {
    let map = crate_def_map(db, krate);
    matches!(map.module_origin(db, map.root_module_id()), crate::nameres::ModuleOrigin::CrateRoot { .. })
}

/// Asking where a module comes from does not run again when a name is added to it: the origin is a field of the cell that
/// a name does not change.
#[test]
fn asking_where_a_module_comes_from_does_not_run_again_when_a_name_is_added_to_it() {
    use salsa::Setter as _;
    let fixture = r#"
//- /a.rs crate:a
pub fn stays() {}

#[cfg(flag)]
pub fn appears() {}
"#;
    let (mut db, _) = TestDB::with_many_files(fixture);
    let krate = local_crates(&db)[0];
    assert!(asks_where_the_root_comes_from(&db, krate), "not vacuous: the root is the crate's root");
    let mut flipped = krate.cfg_options(&db).clone();
    flipped.insert_atom(Symbol::intern("flag"));
    krate.set_cfg_options(&mut db).to(flipped);
    assert!(value_named(&db, krate, "appears").is_some(), "not vacuous: the flip added a name to the root");
    let executed = db.log_executed(|| {
        assert!(asks_where_the_root_comes_from(&db, krate));
    });
    assert!(
        !executed.iter().any(|event| event.contains("asks_where_the_root_comes_from")),
        "what asked where the module comes from was run again, for a name it did not ask for: {executed:?}"
    );
}

/// What the first map of a crate made after a restore says changed is what changed: a name that the file no longer has is
/// said to have changed, so what read it runs again and no longer finds it. (The restore says nothing changed that did not,
/// and everything that did.)
#[test]
fn a_name_that_a_file_loses_after_a_restore_makes_what_read_it_run_again() {
    let fixture = r#"
//- /inner.rs crate:inner
pub fn helper() {}
//- /gated.rs crate:gated deps:inner
pub fn ordinary() { inner::helper(); }
"#;
    let (mut db, files) = TestDB::with_many_files(fixture);
    let crates = local_crates(&db);
    let gated = *crates.iter().find(|krate| value_named(&db, **krate, "ordinary").is_some()).expect("crate gated");
    assert!(resolves_a_path_into_another_crate(&db, gated), "not vacuous: the path resolves");

    super::super::previous::forget_all(&db);
    crate::nameres::seed_previous_maps_from_cells(&mut db);
    db.set_file_text(files[0].file_id(&db), "pub fn renamed() {}\n");

    let executed = db.log_executed(|| {
        assert!(!resolves_a_path_into_another_crate(&db, gated), "`helper` is gone, so the path resolves to nothing");
    });
    assert!(
        executed.iter().any(|event| event.contains("resolves_a_path_into_another_crate")),
        "what read `helper` was not run again after the file lost it: {executed:?}"
    );
}
