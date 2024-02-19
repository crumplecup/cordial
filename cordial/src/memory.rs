//! The `memory` module provides the [`Memorable`] trait, which enables methods for persisting data
//! in a database using a standard CRUD API.
use crate::prelude::*;
#[cfg(feature = "uuids")]
#[cfg_attr(docsrs, doc(cfg(feature = "uuids")))]
use uuid::Uuid;

#[cfg(feature = "memory")]
#[cfg_attr(docsrs, doc(cfg(feature = "memory")))]
#[cfg(feature = "uuids")]
#[cfg_attr(docsrs, doc(cfg(feature = "uuids")))]
#[async_trait::async_trait]
pub trait Memorable<T>: Send + Sync + 'static {
    async fn get(&self, id: Uuid) -> Polite<T>;
    async fn get_all(&self) -> Polite<Vec<T>>;
    async fn create(&self, mem: &T) -> Polite<T>;
    async fn update(&self, mem: &T) -> Polite<T>;
    async fn delete(&self, mem: &T) -> Polite<()>;
}
