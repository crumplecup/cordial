//! Lexical path matching; policy resolution never reads or reparses source.

use std::path::{Component, Path, PathBuf};

#[tracing::instrument(level = "trace", skip(path))]
pub(super) fn normalized(path: &Path) -> Option<PathBuf> {
    let slash_path = path.to_string_lossy().replace('\\', "/");
    let mut result = PathBuf::new();
    for component in Path::new(&slash_path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return None;
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    Some(result)
}

#[tracing::instrument(level = "trace")]
pub(super) fn matches(root: &Path, configured: &str, absolute: &Path) -> Option<usize> {
    if configured.is_empty() {
        return None;
    }
    let relative = normalized(Path::new(configured))?;
    if relative.is_absolute() || configured.split(['/', '\\']).any(|part| part == "..") {
        return None;
    }
    let prefix = normalized(&root.join(relative))?;
    let file = normalized(absolute)?;
    file.starts_with(&prefix)
        .then(|| prefix.components().count())
}
