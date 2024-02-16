use crate::prelude::*;
use tracing::info;
#[cfg(feature = "route")]
use axum::extract::{Path, State};
#[cfg(feature = "route")]
use axum::http::StatusCode;
#[cfg(feature = "route")]
use axum::Json;
#[cfg(feature = "route")]
use axum::response::IntoResponse;
#[cfg(feature = "serial")]
use serde_json::Value;
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
        info!("Getting guest {}", &id);
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
        info!("Getting all guests.");
        let recall = Recall::new(data);
        let guests = recall.get_all().await;
        match guests {
            Ok(result) => Ok((StatusCode::OK, Json(result))),
            Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
        }
    }

    pub async fn check_guest(
        State(data): State<PgPool>,
        Json(guest): Json<Guest>,
        ) -> impl IntoResponse {
        let _ = Recall::new(data);
        info!("Guest info:");
        info!("{:#?}", &guest);
        "Guest checked."
    }

    // pub async fn check_in(
    //     State(data): State<Recall>,
    //     Json(guest): Json<Value>,
    // ) -> Result<impl IntoResponse, impl IntoResponse> {
    //     info!("Receiving request to create {:#?}", &guest);
    //     match guest.clone() {
            // Value::Array(guest_vec) => {
            //     if !guest_vec.is_empty() {
            //         info!("Creating guest {:#?}.", &guest_vec[0].get("name"));
            //         let (_, username) = prune_name(&user_vec[0]["username"].to_string()).unwrap();
            //         let (_, password) = prune_name(&user_vec[0]["password_hash"].to_string()).unwrap();
            //         let usr = User::new(&username, &password);
            //         // let usr = User::new(
            //         //     user_vec[0]["username"].to_string().as_str(),
            //         //     &user_vec[0]["password"].to_string().as_str(),
            //         // );
            //         match data.create(&usr).await {
            //             Ok(result) => Ok((StatusCode::CREATED, Json(result))),
            //             Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
            //         }
            //     } else {
            //         Err((StatusCode::BAD_REQUEST, "User not created.".to_string()))
            //     }
            // }
    //         Value::Object(guest_map) => {
    //             info!("Creating user {:#?}.", &guest_map["username"]);
    //             let (_, name) = prune_name(&guest_map["name"].to_string()).unwrap();
    //             let (_, pass) = prune_name(&guest_map["hash"].to_string()).unwrap();
    //             let guest = Guest::new(&name, &pass);
    //             let res = data.create(&guest).await;
    //             match res {
    //                 Ok(result) => Ok((StatusCode::CREATED, Json(result))),
    //                 Err(e) => Err((StatusCode::BAD_REQUEST, e.to_string())),
    //             }
    //         }
    //         _ => {
    //             info!("Not an array or object variant.");
    //             Err((StatusCode::BAD_REQUEST, "Not an array or object variant.".to_string()))
    //         }
    //     }
    // }

}

