//! Verify the linked allocator behavior; declarations/features are not inferred from text.
use std::error::Error;

#[test]
fn linked_allocator_counts_without_a_profile_guard() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("CORDIAL_ALLOCATOR_FIXTURE").is_some() {
        let before = cordial::live_bytes().ok_or("live meter unavailable")?;
        let bytes = std::hint::black_box(vec![41u8; 123_457]);
        let during = cordial::live_bytes().ok_or("live meter unavailable")?;
        assert!(during >= before + 123_457);
        drop(bytes);
        let after = cordial::live_bytes().ok_or("live meter unavailable")?;
        assert!(after <= during - 123_457);
    } else {
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "linked_allocator_counts_without_a_profile_guard"])
            .env("CORDIAL_ALLOCATOR_FIXTURE", "1")
            .env_remove("CORDIAL_PERF_PROFILE_DIR")
            .status()?;
        assert!(status.success());
    }
    Ok(())
}
