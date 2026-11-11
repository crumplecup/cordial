fn gather() -> usize {
    1
}

/// Names that shape the build.
const NAMES: &[&str] = &[
    "alpha",
    "beta",
];

fn matches_name(name: &str) -> bool {
    NAMES.iter().any(|candidate| name.starts_with(candidate))
}
