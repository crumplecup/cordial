//! Minimal `elicit_url` shadow crate for pipeline test fixtures.
//!
//! Its only role in the fixture is to exist as a workspace member: `url`'s
//! `TRACKED_TARGETS` entry (`shadow: "elicit_url"`) requires a mirror crate
//! by that name to be present before `active_impl_targets()`/
//! `active_tracked_targets()` consider `url` an active target.

/// Demo type mirroring `url::Widget`, present only so this crate isn't empty.
pub struct Widget;
