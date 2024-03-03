//! The `posture` crate contains the database configuration "posture" of the host. Currently
//! supports local postgres hosting.
use dotenvy::dotenv;
use polite::Polite;
use secrecy::ExposeSecret;
use secrecy::Secret;
use serde::Deserialize;
use serde_aux::field_attributes::deserialize_number_from_string;
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use sqlx::ConnectOptions;
use sqlx::{postgres::PgPoolOptions, Connection, Executor, PgConnection, PgPool};
use std::time::Duration;
use tracing::trace;

/// The `Posture` struct contains fields and methods for managing database configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Posture {
    /// The `name` field represents the username of the postgres database.
    pub name: String,
    /// The `pass` field represents the password of the postgres database.
    pub pass: Secret<String>,
    /// The `host` field is a string representation of the database host name.
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub host: String,
    /// The `port` field contains the port number of the database connection.
    pub port: u16,
    /// The `database` field is a string representation of the database name.
    pub database: String,
    /// The `ssl` field indicates whether to require an SSL connection.
    pub ssl: bool,
}

impl Posture {
    /// The `from_env` method politely attempts to create a new `Posture` from the `.env` file in the
    /// working directory.  Commits a [`FauxPas`] if `.env` is not present, or if the variables
    /// `DB_USERNAME`, `DB_PASSWORD`, `DB_HOST` and `DB_NAME` are not present.
    pub fn from_env() -> Polite<Self> {
        dotenv().ok();
        let name = std::env::var("DB_USERNAME")?;
        let pass = std::env::var("DB_PASSWORD")?.into();
        let host = std::env::var("DB_HOST")?;
        let port = 5432;
        let database = std::env::var("DB_NAME")?;
        let ssl = false;
        Ok(Self {
            name,
            pass,
            host,
            port,
            database,
            ssl,
        })
    }

    /// Reveals the connection string for the database.
    pub fn introduction(&self) -> Secret<String> {
        Secret::new(format!(
            "postgres://{}:{}@{}:{}/{}",
            self.name,
            self.pass.expose_secret(),
            self.host,
            self.port,
            self.database
        ))
    }

    /// Reveals the connection string without the database name.
    pub fn intro(&self) -> Secret<String> {
        Secret::new(format!(
            "postgres://{}:{}@{}:{}",
            self.name,
            self.pass.expose_secret(),
            self.host,
            self.port
        ))
    }

    /// Creates a [`PgConnectOptions`] from the field values in the `Posture`, excluding the
    /// database name.
    pub fn connect(&self) -> PgConnectOptions {
        let ssl = if self.ssl {
            PgSslMode::Require
        } else {
            PgSslMode::Prefer
        };
        PgConnectOptions::new()
            .host(&self.host)
            .username(&self.name)
            .password(self.pass.expose_secret())
            .port(self.port)
            .ssl_mode(ssl)
            .application_name("cordial")
    }

    /// Creates a [`PgConnectOptions`] from the field values in the `Posture`, including the
    /// database name.
    pub fn database(&self) -> PgConnectOptions {
        self.connect()
            .database(&self.database)
            .log_statements(tracing::log::LevelFilter::Trace)
    }

    /// Creates a database from a given `Posture`.  Commits a [`FauxPas`] if unable to connect with
    /// Postgres or create the new database.
    pub async fn create(&self) -> Polite<()> {
        trace!("Creating database {}.", &self.database);
        let mut connection = PgConnection::connect_with(&self.connect()).await?;
        connection
            .execute(&*format!(r#"CREATE DATABASE "{}";"#, self.database))
            .await?;
        Ok(())
    }

    pub async fn migrate(&self) -> Polite<()> {
        let connection_pool = PgPool::connect_with(self.database()).await?;
        trace!("Migrating database.");
        sqlx::migrate!("./migrations").run(&connection_pool).await?;
        Ok(())
    }

    pub async fn delete(&self) -> Polite<()> {
        trace!("Deleting database {}.", &self.database);
        let mut connection = PgConnection::connect_with(&self.connect()).await?;
        connection
            .execute(&*format!(r#"DROP DATABASE "{}";"#, self.database))
            .await?;
        Ok(())
    }

    pub async fn try_delete(&self) -> Polite<()> {
        trace!("Attempting to delete database {}.", &self.database);
        let mut connection = PgConnection::connect_with(&self.connect()).await?;
        match connection
            .execute(&*format!(r#"DROP DATABASE "{}";"#, self.database))
            .await
        {
            Ok(res) => {
                trace!("Database {} deleted.", self.database);
                trace!("Rows affected: {}", res.rows_affected());
            }
            Err(e) => trace!(
                "Failed to delete {}.  Error: {}.",
                self.database,
                e.to_string()
            ),
        }
        Ok(())
    }

    pub fn book(&self) -> PgPool {
        trace!("Creating connection pool.");
        PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .connect_lazy_with(self.database())
    }
}
