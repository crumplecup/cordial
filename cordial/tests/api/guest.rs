use crate::prelude::*;
use cordial::prelude::*;
use axum::body::Body;
use axum::http::{self, Request};
use axum::Router;
use axum::routing::get;
use http_body_util::BodyExt;
use tokio::net::TcpListener;
use tower::{Service, ServiceExt};
use tracing::{info, trace};

pub async fn create_guest(host: &Host) -> Polite<()> {
    let mut improv = Improv::default();
    let mut guest = improv.guest()?;
    trace!("Guest: {:#?}", &guest);
    let recall = recall(&host).await;
    let created = recall.create(&guest).await?;
    assert_eq!(&guest, &created);
    guest.name = improv.name()?;
    guest.hash = improv.pass()?;
    let updated = recall.update(&guest).await?;
    assert_eq!(&guest, &updated);
    recall.delete(&guest).await?;

    Ok(())
}

pub async fn guest_check(host: &mut Host) -> Polite<()> {
    let mut improv = Improv::default();
    let mut guest = improv.guest()?;
    info!("Guest: {:#?}", &guest);
    let recall = recall(&host).await;
    let created = recall.create(&guest).await?;
    assert_eq!(&guest, &created);
    bearing(host).await?;
    let uri = format!("/lookup/{}", &guest.id);
    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = host.bearing.cues.clone();
    let app = Router::new()
        .route("/book", get(Counsel::book))
        .with_state(recall.book.clone());
    // tokio::spawn(async move {
    //         axum::serve(listener, app);
    //     });
    let response = app.oneshot(Request::builder()
                                   .uri(&uri)
                                   .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                                   .body(Body::empty())?)
                                   .await
                                   .unwrap();


    Ok(())

}
