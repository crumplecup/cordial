// The digest of the sources that shape a cached analysis.
//
// Read by `build.rs`, which bakes the result into the executable, and by
// `tests/run_cache_scope.rs`, which checks what moves it. It needs
// `OUTSIDE_THE_ANALYSIS` and `sha2` in scope and nothing else of the crate.

/// Whether `relative`, a path under `src/` written with `/`, is left out.
pub fn is_outside_the_analysis(relative: &str) -> bool {
    OUTSIDE_THE_ANALYSIS
        .iter()
        .any(|entry| relative == *entry || (entry.ends_with('/') && relative.starts_with(entry)))
}

/// Hash every `.rs` file under `src_root` that the key does not ignore: its path
/// and its bytes, in path order. A file added, removed, renamed or edited moves
/// the digest; one on the ignored list does not.
pub fn analysis_sources_digest(src_root: &std::path::Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    fn walk(
        directory: &std::path::Path,
        root: &std::path::Path,
        found: &mut Vec<(String, std::path::PathBuf)>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(&path, root, found)?;
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let relative = path
                    .strip_prefix(root)
                    .map_err(std::io::Error::other)?
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push((relative, path));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(src_root, src_root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        if is_outside_the_analysis(&relative) {
            continue;
        }
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(std::fs::read(&path)?);
        hasher.update([0]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
