//! Record the features of the executable that actually produced a profile.

fn main() {
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        && output.status.success()
    {
        println!(
            "cargo:rustc-env=CORDIAL_BUILD_REVISION={}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
    }
    if let Ok(output) = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        && output.status.success()
    {
        println!(
            "cargo:rustc-env=CORDIAL_BUILD_DIRTY={}",
            !output.stdout.is_empty()
        );
    }
    let mut features: Vec<_> = std::env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .collect();
    features.sort();
    println!(
        "cargo:rustc-env=CORDIAL_BUILD_FEATURES={}",
        features.join(",")
    );
}
