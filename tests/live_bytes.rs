//! The bytes the program holds, as the allocator counts them, follow what is allocated and freed
//! at once, whatever the allocator does with the pages: the meter the memory limit is judged by.
//!
//! The resident memory does not: an allocator may keep what is freed. A run that held 240 MB read as
//! 1.5 GB resident, and no amount of freeing could bring a limit on that reading back under it.

const OBJECT_BYTES: usize = 96;
const OBJECTS: usize = 1_000_000;

fn live() -> u64 {
    cordial::live_bytes().unwrap_or_else(|| panic!("the allocator counts"))
}

#[test]
fn a_large_allocation_is_counted_and_gone_when_freed() {
    let before = live();
    let held = vec![7_u8; 64 << 20];
    let grown = live().saturating_sub(before);
    assert!(grown >= (64 << 20), "counted while held: {grown}");
    drop(held);
    let kept = live().saturating_sub(before);
    assert!(kept < (1 << 20), "and not once freed: {kept}");
}

/// Small objects freed from among others that stay: the pattern that leaves an allocator's pages
/// partly used and resident. What is counted is what is held.
#[test]
fn small_objects_freed_from_among_those_that_stay_are_counted_as_freed() {
    let before = live();
    let mut objects: Vec<Option<Box<[u8; OBJECT_BYTES]>>> = (0..OBJECTS)
        .map(|index| {
            Some(Box::new(
                [u8::try_from(index % 250).unwrap_or(1) + 1; OBJECT_BYTES],
            ))
        })
        .collect();
    let grown = live().saturating_sub(before);
    assert!(
        grown >= u64::try_from(OBJECTS * OBJECT_BYTES).unwrap_or(0),
        "counted while held: {grown}"
    );
    // Seven of every eight are freed, and one stays between them.
    for (index, slot) in objects.iter_mut().enumerate() {
        if index % 8 != 0 {
            *slot = None;
        }
    }
    let kept = live().saturating_sub(before);
    let stays = u64::try_from(OBJECTS / 8 * (OBJECT_BYTES + 16)).unwrap_or(0);
    assert!(
        kept < stays * 3 / 2,
        "about an eighth is still held: {kept} bytes against {stays}"
    );
    assert!(objects.iter().flatten().count() == OBJECTS / 8);
}
