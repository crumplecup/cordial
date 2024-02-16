//! The `recall` module contains the [`Recall`] struct, with methods for
//! constructing a handle for accessing a Postgres database connection pool.  The [`Recall`] struct
//! implements the [`Memorable`] trait for the type [`Guest`], to enable CRUD access to the
//! database for managing [`Guest`] data.
use crate::prelude::*;
use tracing::trace;
#[cfg(feature = "sql")]
use sqlx::PgPool;

/// The `Recall` struct holds memories of each guest ([`crate::guest::Guest`]) using a handle to a database `book`.
#[cfg(feature = "sql")]
#[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
#[derive(Debug, Clone)]
pub struct Recall {
    /// The `book` field holds a handle to a pool of database connections.
    pub book: PgPool,
}

impl Recall {
    /// Creates a new `Recall` using `book`, a handle to the database.
    pub fn new(book: PgPool) -> Self {
        Self { book }
    }
}

impl From<Posture> for Recall {
    fn from(posture: Posture) -> Self {
        let book = posture.book();
        Recall::new(book)
    }

}

impl From<&Posture> for Recall {
    fn from(posture: &Posture) -> Self {
        let book = posture.book();
        Recall::new(book)
    }

}

#[async_trait::async_trait]
impl Memorable<Guest> for Recall {

    async fn get(&self, id: uuid::Uuid) -> Polite<Guest> {
        trace!("Calling get() for id {}", &id);
        Ok(sqlx::query_as::<_, Guest>(
            r#"
      SELECT id, name, hash
      FROM guests
      WHERE id = $1
      "#,
            // RETURNING (id, username, hash)
        )
        .bind(id)
        .fetch_one(&self.book)
        .await?)
    }

    async fn get_all(&self) -> Polite<Vec<Guest>> {
        let req = sqlx::query_as::<_, Guest>(
            r#"
      SELECT id, name, hash
      FROM guests
      "#,
        )
        .fetch_all(&self.book)
        .await?;
        Ok(req)
    }

    async fn create(&self, mem: &Guest) -> Polite<Guest> {
        trace!("Calling create for {}.", &mem.name);
        let req = sqlx::query_as::<_, Guest>(
            r#"
      INSERT INTO guests (id, name, hash)
      VALUES ($1, $2, $3)
      RETURNING id, name, hash
      "#,
        )
        .bind(mem.id)
        .bind(&mem.name)
        .bind(&mem.hash)
        .fetch_one(&self.book)
        .await?;
        Ok(req)
    }

    async fn update(&self, mem: &Guest) -> Polite<Guest> {
        trace!("Calling update for id {}", &mem.id);
        let req = sqlx::query(
            r#"
      UPDATE guests
      SET name = $1, hash = $2
      WHERE id = $3
      "#,
        )
        .bind(&mem.name)
        .bind(&mem.hash)
        .bind(mem.id)
        .execute(&self.book)
        .await?;
        trace!("{:#?}", &req);

        Ok(mem.clone())
    }

    async fn delete(&self, mem: &Guest) -> Polite<()> {
        trace!("Calling delete for id {}", &mem.id);
        let req = sqlx::query::<_>(
            r#"
      DELETE from guests
      WHERE id = $1
      "#,
        )
        .bind(mem.id)
        .execute(&self.book)
        .await?;
        trace!("{:#?}", &req);
        Ok(())
    }

}
