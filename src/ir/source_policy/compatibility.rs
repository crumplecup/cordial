//! Temporary collectors sharing the shipping loader's single syn tree.

#[cfg(feature = "allows")]
mod allows;
#[cfg(feature = "cfg_hygiene")]
mod cfg_hygiene;
#[cfg(feature = "cfg_scatter")]
mod cfg_scatter;

#[cfg(feature = "allows")]
pub(super) use allows::collect as collect_allows;
#[cfg(feature = "allows")]
pub use allows::scan_rust_source as scan_allows_source;
#[cfg(feature = "cfg_hygiene")]
pub(super) use cfg_hygiene::collect as collect_cfg_hygiene;
#[cfg(feature = "cfg_hygiene")]
pub use cfg_hygiene::scan_rust_source as scan_cfg_hygiene_source;
#[cfg(feature = "cfg_scatter")]
pub(super) use cfg_scatter::collect as collect_cfg_scatter;
#[cfg(feature = "cfg_scatter")]
pub use cfg_scatter::scan_rust_source as scan_cfg_scatter_source;
