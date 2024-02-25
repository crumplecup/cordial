//! The `guest` module holds the [`Guest`] struct for managing multiple users in an application.

/// The `Guest` struct provides convenience methods around user management.
// #[cfg(feature = "serial")]
#[cfg(feature = "uuids")]
#[cfg_attr(docsrs, doc(cfg(feature = "serial")))]
#[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
#[cfg_attr(docsrs, doc(cfg(feature = "uuids")))]
#[cfg_attr(feature = "serial", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "sql", derive(sqlx::FromRow))]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Guest {
    /// The `id` field holds an identifier unique to the `Guest`.
    pub id: uuid::Uuid,
    /// The `name` field holds the name of the `Guest`.
    pub name: String,
    /// The `hash` field holds the hashed password of the `Guest`.
    pub hash: String,
}

impl Guest {
    /// Create a new `Guest` from a given `name` and password `pass`.
    #[cfg(feature = "uuids")]
    #[cfg_attr(docsrs, doc(cfg(feature = "uuids")))]
    pub fn new(name: &str, pass: &str) -> Self {
        let id = uuid::Uuid::new_v4();
        Guest {
            id,
            name: name.to_owned(),
            hash: pass.to_owned(),
        }
    }
}
