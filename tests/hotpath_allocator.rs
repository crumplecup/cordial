//! Allocation profiling needs one counting allocator in the program. Cordial
//! declares it once, in the library, under the `hotpath-alloc` feature, so the CLI
//! and every test binary that links the library are counted without a declaration
//! of their own (a second would not link).

use std::path::Path;

fn sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    ["src", "tests", "examples"]
        .iter()
        .flat_map(|dir| walkdir::WalkDir::new(root.join(dir)).into_iter())
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            Some((entry.path().display().to_string(), text))
        })
        .collect()
}

#[test]
fn the_counting_allocator_is_declared_exactly_once_and_behind_its_feature() {
    let declared: Vec<_> = sources()
        .into_iter()
        .filter(|(path, _)| !path.ends_with("tests/hotpath_allocator.rs"))
        .flat_map(|(path, text)| {
            text.lines()
                .enumerate()
                .filter(|(_, line)| line.trim() == "#[global_allocator]")
                .map(|(index, _)| format!("{path}:{}", index + 1))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        declared.len(),
        1,
        "one allocator for the whole program: {declared:?}"
    );
    assert!(
        declared[0].contains("src/profiling.rs"),
        "the profiling module holds it: {declared:?}"
    );
    let profile =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/profiling.rs"))
            .unwrap_or_default();
    let at = profile.find("#[global_allocator]").unwrap_or_default();
    assert!(
        profile[..at]
            .lines()
            .rev()
            .take(3)
            .any(|line| line.contains("feature = \"hotpath-alloc\"")),
        "the allocator is only declared when allocation profiling is on"
    );
}

#[test]
fn the_manifest_forwards_the_feature_to_hotpath() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .unwrap_or_default();
    assert!(
        manifest.contains("hotpath-alloc = [\"hotpath\", \"hotpath/hotpath-alloc\"]"),
        "hotpath-alloc turns on hotpath's counting"
    );
}

#[test]
fn one_function_builds_every_profile_guard() {
    let builders: Vec<_> = sources()
        .into_iter()
        .filter(|(path, _)| path.contains("/src/"))
        .flat_map(|(path, text)| {
            text.lines()
                .enumerate()
                .filter(|(_, line)| line.contains("HotpathGuardBuilder::new"))
                .map(|(index, _)| format!("{path}:{}", index + 1))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        builders.len(),
        1,
        "build guards through profiling::guard: {builders:?}"
    );
    assert!(builders[0].contains("src/profiling.rs"), "{builders:?}");
}
