use cordial::prelude::*;
use crate::prelude::*;
use axum::routing::get;
use secrecy::ExposeSecret;
use std::sync::Arc;
use tracing::{info, trace};

pub async fn local_posture(host: &Host) -> Polite<()> {
    trace!(
        "Connection: {}",
        host.posture.introduction().expose_secret()
    );
    host.posture.try_delete().await?;
    trace!(
        "Database deleted at connection {}.",
        host.posture.introduction().expose_secret()
    );
    host.posture.create().await?;
    trace!(
        "Database created at connection {}.",
        host.posture.introduction().expose_secret()
    );
    host.posture.migrate().await?;
    trace!(
        "Database migrated at connection {}.",
        host.posture.introduction().expose_secret()
    );
    info!("Local posture test successful.");
    Ok(())
}

pub async fn bearing(host: &mut Host) -> Polite<()> {
    local_posture(&host).await?;
    host.bearing.cues = host.bearing.cues.clone()
        .route("/book", get(Counsel::book))
        .route("/lookup/:id", get(Counsel::lookup))
        .route("/lookup_all", get(Counsel::lookup_all))
        .route("/check", get(Counsel::check_guest));
    trace!("Bearing: {:#?}", host.bearing);
    Ok(())
}

pub async fn recall(host: &Host) -> Recall {
    Recall::from(&host.posture)
}

pub async fn host() -> Polite<Host> {
    Host::from_env().await
}


