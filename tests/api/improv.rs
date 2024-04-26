use axum::body::Body;
use axum::http::{self, Request};
use cordial::prelude::*;
use http_body_util::BodyExt;
use tower::ServiceExt;
use tracing::info;

pub async fn improvise(host: &Host) -> Polite<()> {
    info!("Checking name recommendations.");
    guest_name(host).await?;
    info!("Name recommendations acceptable.");
    info!("Checking pass recommendations.");
    guest_pass(host).await?;
    pass_adv(host).await?;
    info!("Pass recommendations acceptable.");
    Ok(())
}

async fn guest_name(host: &Host) -> Polite<()> {
    let app = host.bearing();
    let uri = format!("/improv/name");
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
    info!("New name: {:#?}", &body);

    let uri = format!("/improv/name/num");
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
    info!("New numbered name: {:#?}", &body);
    Ok(())
}

async fn guest_pass(host: &Host) -> Polite<()> {
    let app = host.bearing();
    let uri = format!("/improv/pass");
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
    info!("New pass: {:#?}", &body);
    Ok(())
}

async fn pass_adv(host: &Host) -> Polite<()> {
    let app = host.bearing();
    let uri = format!("/improv/pass");
    let mut pass = Pass::new();
    pass.length = 20;
    pass.numbers = false;
    pass.lowercase = false;
    pass.uppercase = true;
    pass.symbols = true;
    pass.spaces = true;
    pass.exclude = true;
    pass.strict = true;

    let body = serde_json::json!(&pass);
    let body = serde_json::to_vec(&body)?;
    let response = app
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
    info!("New pass: {:#?}", &body);
    Ok(())
}
