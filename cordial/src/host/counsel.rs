use crate::prelude::*;
use tracing::trace;
#[cfg(feature = "route")]
use axum::extract::{Path, State};
#[cfg(feature = "route")]
use axum::http::StatusCode;
#[cfg(feature = "route")]
use axum::Json;
#[cfg(feature = "route")]
use axum::response::IntoResponse;
#[cfg(feature = "sql")]
use sqlx::PgPool;

#[cfg(feature = "route")]
#[cfg_attr(docsrs, doc(cfg(feature = "route")))]
#[cfg(feature = "sql")]
#[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
#[derive(Copy, Clone, Debug, Default)]
pub struct Counsel;

impl Counsel {
    pub fn new() -> Self {
        Default::default()
    }

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

    pub async fn check() -> impl IntoResponse {
        tracing::info!("Bearing check.");
        StatusCode::OK
    }

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
}
