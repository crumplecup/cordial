use axum::body::Body;
use axum::http::{self, Request};
use cordial::prelude::*;
use http_body_util::BodyExt;
// use tokio::net::TcpListener;
// use tower::{Service, ServiceExt};
use tower::ServiceExt;
use tracing::{info, trace};

pub async fn booking(host: &Host) -> Polite<()> {
    let mut improv = Improv::default();
    let mut guest = improv.guest()?;
    info!("Guest: {:#?}", &guest);
    let created = host.recall.create(&guest).await?;
    assert_eq!(&guest, &created);
    guest.name = improv.name()?;
    guest.hash = improv.pass()?;
    let updated = host.recall.update(&guest).await?;
    assert_eq!(&guest, &updated);
    host.recall.delete(&guest).await?;
    Ok(())
}

pub async fn guest_check(host: &mut Host) -> Polite<()> {
    let mut improv = Improv::default();
    let guest = improv.guest()?;
    trace!("Guest: {:#?}", &guest);
    let created = host.recall.create(&guest).await?;
    assert_eq!(&guest, &created);
    let app = host.bearing();
    // tokio::spawn(async move {
    //         axum::serve(listener, app);
    //     });
    let uri = format!("/guests");
    let response = app
        .oneshot(
            Request::builder()
                .uri(&uri)
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await?.to_bytes();
    let body: Vec<Guest> = serde_json::from_slice(&body).unwrap();
    trace!("{:#?}", body);
    assert_eq!(&guest, &body[0]);
    info!("Lookup all guests successful.");
    Ok(())
}

pub async fn guest_lifecycle(host: &mut Host) -> Polite<()> {
    info!("Testing guest lifecycle.");
    let mut improv = Improv::default();
    let mut guest = improv.guest()?;
    // let created = host.recall.create(&guest).await?;
    // assert_eq!(&guest, &created);
    // info!("Guest {} created.", &guest.name);
    let app = host.bearing();

    info!("Creating guest {}.", &guest.name);
    let uri = format!("/guests");
    let body = serde_json::json!(&guest);
    let body = serde_json::to_vec(&body)?;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&uri)
                .method(http::Method::POST)
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(body))?,
        )
        .await?;
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await?.to_bytes();
    let body: Guest = serde_json::from_slice(&body).unwrap();
    assert_eq!(&guest, &body);
    info!("Guest creation successful for {}.", &guest.name);

    info!("Looking up guest {}.", &guest.name);
    let uri = format!("/guests/{}", &guest.id);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&uri)
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await?.to_bytes();
    let body: Guest = serde_json::from_slice(&body).unwrap();
    assert_eq!(&guest, &body);
    info!("Guest lookup successful for {}.", &guest.name);

    info!("Updating name and hash for guest id {}.", &guest.id);
    guest.name = improv.name()?;
    guest.hash = improv.pass()?;
    let uri = format!("/guests/{}", &guest.id);
    let body = serde_json::json!(&guest);
    let body = serde_json::to_vec(&body)?;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&uri)
                .method(http::Method::PUT)
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(body))?,
        )
        .await?;
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await?.to_bytes();
    let body: Guest = serde_json::from_slice(&body).unwrap();
    assert_eq!(&guest, &body);
    info!("Guest update successful for {}.", &guest.name);

    info!("Checking out {}.", &guest.name);
    let uri = format!("/guests/{}", &guest.id);
    let body = serde_json::json!(&guest);
    let body = serde_json::to_vec(&body)?;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&uri)
                .method(http::Method::DELETE)
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(body))?,
        )
        .await?;
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await?.to_bytes();
    assert!(body.is_empty());
    info!("Guest {} successfully checked out.", &guest.name);

    Ok(())
}
