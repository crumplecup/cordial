//! Shared architecture catalog; policy reads the retained catalog.

mod catalog;
mod lint;

pub(crate) use catalog::Catalog as ErrorArchitectureCatalog;
