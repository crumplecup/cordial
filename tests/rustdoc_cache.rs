#![cfg(feature = "rustdoc")]

use std::time::Duration;

use cordial::{CordialResult, testing::rustdoc_cache_is_fresh};

#[test]
fn rustdoc_cache_freshness_tracks_member_sources() -> CordialResult<()> {
    let temp = tempfile::tempdir()?;
    let project = temp.path();
    let src = project.join("src");
    std::fs::create_dir_all(&src)?;
    let source = src.join("lib.rs");
    let cache = project.join("cache.json");

    std::fs::write(project.join("Cargo.toml"), "[package]\nname = \"demo\"\n")?;
    std::fs::write(&source, "pub struct Before;\n")?;
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(&cache, "{}\n")?;

    assert!(rustdoc_cache_is_fresh(project, project, &cache));

    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(&source, "pub struct After;\n")?;

    assert!(!rustdoc_cache_is_fresh(project, project, &cache));
    Ok(())
}
