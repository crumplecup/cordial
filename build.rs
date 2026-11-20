//! Record the features of the executable that actually produced a profile.

fn main() {
    let mut features: Vec<_> = std::env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .collect();
    features.sort();
    println!(
        "cargo:rustc-env=CORDIAL_BUILD_FEATURES={}",
        features.join(",")
    );
}
