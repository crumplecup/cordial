//! The `memory` crate provides the [`Memorable`] trait, which enables methods for persisting data
//! in a database using a standard CRUD API.
use polite::Polite;
use uuid::Uuid;

/// The `Memorable` trait defines a protocol for creating, reading, updating and deleting users
/// from a database.
#[async_trait::async_trait]
pub trait Memorable<T>: Send + Sync + 'static {
    async fn get(&self, id: Uuid) -> Polite<T>;
    async fn get_all(&self) -> Polite<Vec<T>>;
    async fn create(&self, mem: &T) -> Polite<T>;
    async fn update(&self, mem: &T) -> Polite<T>;
    async fn delete(&self, mem: &T) -> Polite<()>;
}
