//! The `counsel` module offers directions and recommendations to a [`Guest`].
use crate::prelude::*;
#[cfg(feature = "route")]
use axum::extract::{Path, State};
#[cfg(feature = "route")]
use axum::http::StatusCode;
#[cfg(feature = "route")]
use axum::response::IntoResponse;
#[cfg(feature = "route")]
use axum::Json;
#[cfg(feature = "sql")]
use sqlx::PgPool;
use tracing::trace;

/// The `Counsel` struct holds methods related to offering directions and recommendations to a
/// [`Guest`].
#[cfg(feature = "route")]
#[cfg_attr(docsrs, doc(cfg(feature = "route")))]
#[cfg(feature = "sql")]
#[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
#[derive(Copy, Clone, Debug, Default)]
pub struct Counsel;

impl Counsel {
    /// Creates a new `Counsel` struct, an empty struct that coordinates methods related to route
    /// handling.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    pub fn new() -> Self {
        Default::default()
    }

    /// The `book` method returns the version of the postgres database if available.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "sql")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
    pub async fn book(State(data): State<PgPool>) -> impl IntoResponse {
        tracing::info!("Getting version");
        let recall = Recall::new(data);
        let result: Result<String, sqlx::Error> = sqlx::query_scalar("SELECT version()")
            .fetch_one(&recall.book)
            .await;

        let ver = match result {
            Ok(version) => version,
            Err(e) => format!("Error: {:?}", e),
        };

        (StatusCode::OK, ver)
    }

    /// The `check` method returns a status OK, used to assess if the system is responsive.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    pub async fn check() -> impl IntoResponse {
        tracing::info!("Bearing check.");
        StatusCode::OK
    }

    /// The `lookup` method looks up a [`Guest`] based upon their `id`.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "sql")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
    pub async fn lookup(
        Path(id): Path<uuid::Uuid>,
        State(data): State<PgPool>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        trace!("Getting guest {}", &id);
        let recall = Recall::new(data);
        let guest = recall.get(id).await;
        match guest {
            Ok(result) => Ok((StatusCode::OK, Json(result))),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `lookup_all` method returns all [`Guest`] entries.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "sql")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
    pub async fn lookup_all(
        State(data): State<PgPool>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        trace!("Getting all guests.");
        let recall = Recall::new(data);
        let guests = recall.get_all().await;
        match guests {
            Ok(result) => Ok((StatusCode::OK, Json(result))),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `check_in` method enters a new [`Guest`] into the book.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "sql")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
    pub async fn check_in(
        State(data): State<PgPool>,
        Json(guest): Json<Guest>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        trace!("Checking in guest {}.", &guest.name);
        let recall = Recall::new(data);
        let attempt = recall.create(&guest).await;
        match attempt {
            Ok(created) => Ok((StatusCode::OK, Json(created))),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `update` method updates the `name` and `hash` fields of a [`Guest`], while maintain the
    /// same `id`.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "sql")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
    pub async fn update(
        State(data): State<PgPool>,
        Json(guest): Json<Guest>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        trace!("Updating guest {}.", &guest.name);
        let recall = Recall::new(data);
        let attempt = recall.update(&guest).await;
        match attempt {
            Ok(updated) => Ok((StatusCode::OK, Json(updated))),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `check_out` method removes a [`Guest`] from the book.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "sql")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
    pub async fn check_out(
        State(data): State<PgPool>,
        Json(guest): Json<Guest>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        trace!("Checking out guest {}.", &guest.name);
        let recall = Recall::new(data);
        let attempt = recall.delete(&guest).await;
        match attempt {
            Ok(()) => Ok(StatusCode::OK),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `guest_name` method offers a recommendation for the `name` of a [`Guest`].
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "improv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "improv")))]
    pub async fn guest_name() -> Result<impl IntoResponse, impl IntoResponse> {
        trace!("Recommending guest name.");
        let mut improv = Improv::new(false);
        let attempt = improv.name();
        match attempt {
            Ok(result) => Ok((StatusCode::OK, result)),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `guest_name_numbered` method offers a recommendation for a numbered `name` of a [`Guest`].
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "improv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "improv")))]
    pub async fn guest_name_numbered() -> Result<impl IntoResponse, impl IntoResponse> {
        trace!("Recommending numbered guest name.");
        let mut improv = Improv::new(true);
        let attempt = improv.name();
        match attempt {
            Ok(result) => Ok((StatusCode::OK, result)),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `guest_pass` method offers a recommendation for the `pass` of a [`Guest`].
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "improv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "improv")))]
    pub async fn guest_pass() -> Result<impl IntoResponse, impl IntoResponse> {
        trace!("Recommending guest pass.");
        let improv = Improv::new(false);
        let attempt = improv.pass();
        match attempt {
            Ok(result) => Ok((StatusCode::OK, result)),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `pass_adv` method offers a recommendation for the `pass` of a [`Guest`] using the
    /// configuration provided in the request body.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[cfg(feature = "improv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "improv")))]
    pub async fn pass_adv(
        Json(config): Json<Pass>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        trace!("Recommending custom pass.");
        let mut improv = Improv::new(false);
        improv.pass = improv
            .pass
            .clone()
            .length(config.length)
            .numbers(config.numbers)
            .lowercase_letters(config.lowercase)
            .uppercase_letters(config.uppercase)
            .symbols(config.symbols)
            .spaces(config.spaces)
            .exclude_similar_characters(config.exclude)
            .strict(config.strict);

        let attempt = improv.pass();
        match attempt {
            Ok(result) => Ok((StatusCode::OK, result)),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }
}
