use cordial::prelude::*;
use axum::body::Body;
use axum::http::{self, Request, Response};
use axum::Router;
use axum::routing::get;
use tokio::net::TcpListener;
use tower::{Service, ServiceExt};
use tracing::{info, warn};

pub async fn fauxpas(mut host: &mut Host) -> Polite<()> {
    let _ = env()?;
    info!("Env test successful.");
    let check = infallible(&mut host).await?;
    info!("{:#?}", check.status());
    info!("{:#?}", check.body());
    info!("Infallibility test successful.");
    Ok(())
}

fn env() -> Polite<()> {
    match not_there() {
        Ok(not_there) => {
            warn!("Should not be reachable: {:#?}",  not_there);
            Err(FauxPas::BadTest)
        },
        Err(FauxPas::Env(_)) => {
            Ok(())
        }
        Err(e) => {
            warn!("Unexpected error: {:#?}", e.to_string());
            Err(FauxPas::BadTest)
        }
    }
}

fn not_there() -> Polite<String> {
    let not_there = std::env::var("NOT_THERE")?;
    Ok(not_there)
}

async fn infallible(host: &mut Host) -> Polite<Response<Body>> {
    let recall: Recall = host.posture.clone().into();
    let app = Router::new()
        .route("/book", get(Counsel::book))
        .with_state(recall.book.clone());

    let uri = format!("/book");
    let response = app.oneshot(Request::builder()
                                   .uri(&uri)
                                   .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                                   .body(Body::empty())?)
                                   .await?;
    Ok(response)
}
