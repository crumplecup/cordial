use std::path::Path;

pub fn missing(path: &Path, report: &str) {
    let _ = (path, report);
}

pub fn traced(path: &Path) {
    let _ = path;
}
