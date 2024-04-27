//! The `counsel` crate offers directions and recommendations to a [`Guest`].
use axum::extract::{Path, State};
use axum::http::header::{HeaderMap, HeaderValue, ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use cordial_guest::Guest;
use cordial_improv::{Improv, Pass};
use cordial_memory::Memorable;
use cordial_recall::Recall;
use sqlx::PgPool;
use tracing::{info, trace};
use uuid::Uuid;

pub const SERVER: &str = "http://127.0.0.1:8000";
pub const CLIENT: &str = "http://127.0.0.1:8080";

/// The `Counsel` struct holds methods related to offering directions and recommendations to a
/// [`Guest`].
#[derive(Copy, Clone, Debug, Default)]
pub struct Counsel;

impl Counsel {
    /// Creates a new `Counsel` struct, an empty struct that coordinates methods related to route
    /// handling.
    pub fn new() -> Self {
        Default::default()
    }

    pub fn headers() -> HeaderMap {
        HeaderMap::new()
    }

    pub fn access(headers: &mut HeaderMap) {
        headers.insert(
            ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static(SERVER),
        );
        headers.insert(
            ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static(CLIENT),
        );
    }

    pub fn plain(headers: &mut HeaderMap) {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    }

    /// The `book` method returns the version of the postgres database if available.
    pub async fn book(State(data): State<PgPool>) -> impl IntoResponse {
        info!("Checking book.");
        trace!("Getting version");
        let mut headers = Counsel::headers();
        Counsel::access(&mut headers);
        Counsel::plain(&mut headers);
        let recall = Recall::new(data);
        let result: Result<String, sqlx::Error> = sqlx::query_scalar("SELECT version()")
            .fetch_one(&recall.book)
            .await;

        let ver = match result {
            Ok(version) => version,
            Err(e) => format!("{:#?}", e),
        };

        (StatusCode::OK, headers, ver)
    }

    /// The `check` method returns a status OK, used to assess if the system is responsive.
    pub async fn check() -> impl IntoResponse {
        info!("Bearing check.");
        let mut headers = Counsel::headers();
        Counsel::access(&mut headers);
        (StatusCode::OK, headers)
    }

    /// The `lookup` method looks up a [`Guest`] based upon their `id`.
    pub async fn lookup(
        Path(id): Path<Uuid>,
        State(data): State<PgPool>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        info!("Getting guest {}", &id);
        let recall = Recall::new(data);
        let guest = recall.get(id).await;
        match guest {
            Ok(result) => Ok((StatusCode::OK, Json(result))),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `lookup_all` method returns all [`Guest`] entries.
    pub async fn lookup_all(
        State(data): State<PgPool>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        info!("Getting all guests.");
        let recall = Recall::new(data);
        let guests = recall.get_all().await;
        match guests {
            Ok(result) => Ok((StatusCode::OK, Json(result))),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `check_in` method enters a new [`Guest`] into the book.
    pub async fn check_in(
        State(data): State<PgPool>,
        Json(guest): Json<Guest>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        info!("Checking in guest {}.", &guest.name);
        let recall = Recall::new(data);
        let attempt = recall.create(&guest).await;
        match attempt {
            Ok(created) => Ok((StatusCode::OK, Json(created))),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `update` method updates the `name` and `hash` fields of a [`Guest`], while maintain the
    /// same `id`.
    pub async fn update(
        State(data): State<PgPool>,
        Json(guest): Json<Guest>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        info!("Updating guest {}.", &guest.name);
        let recall = Recall::new(data);
        let attempt = recall.update(&guest).await;
        match attempt {
            Ok(updated) => Ok((StatusCode::OK, Json(updated))),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `check_out` method removes a [`Guest`] from the book.
    pub async fn check_out(
        State(data): State<PgPool>,
        Json(guest): Json<Guest>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        info!("Checking out guest {}.", &guest.name);
        let recall = Recall::new(data);
        let attempt = recall.delete(&guest).await;
        match attempt {
            Ok(()) => Ok(StatusCode::OK),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `guest_name` method offers a recommendation for the `name` of a [`Guest`].
    pub async fn guest_name() -> Result<impl IntoResponse, impl IntoResponse> {
        info!("Recommending guest name.");
        let mut improv = Improv::new(false);
        let attempt = improv.name();
        match attempt {
            Ok(result) => Ok((StatusCode::OK, result)),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `guest_name_numbered` method offers a recommendation for a numbered `name` of a [`Guest`].
    pub async fn guest_name_numbered() -> Result<impl IntoResponse, impl IntoResponse> {
        info!("Recommending numbered guest name.");
        let mut improv = Improv::new(true);
        let attempt = improv.name();
        match attempt {
            Ok(result) => Ok((StatusCode::OK, result)),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `guest_pass` method offers a recommendation for the `pass` of a [`Guest`].
    pub async fn guest_pass() -> Result<impl IntoResponse, impl IntoResponse> {
        info!("Recommending guest pass.");
        let improv = Improv::new(false);
        let attempt = improv.pass();
        match attempt {
            Ok(result) => Ok((StatusCode::OK, result)),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    /// The `pass_adv` method offers a recommendation for the `pass` of a [`Guest`] using the
    /// configuration provided in the request body.
    pub async fn pass_adv(
        Json(config): Json<Pass>,
    ) -> Result<impl IntoResponse, impl IntoResponse> {
        info!("Recommending custom pass.");
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
