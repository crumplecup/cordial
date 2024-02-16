use cordial::prelude::*;
use crate::prelude::*;
use axum::routing::get;
use secrecy::ExposeSecret;
use std::sync::Arc;
use tracing::{info, trace};

pub async fn local_posture(wild: &WildGuest) -> Polite<()> {
    trace!(
        "Connection: {}",
        wild.posture.introduction().expose_secret()
    );
    wild.posture.try_delete().await?;
    trace!(
        "Database deleted at connection {}.",
        wild.posture.introduction().expose_secret()
    );
    wild.posture.create().await?;
    trace!(
        "Database created at connection {}.",
        wild.posture.introduction().expose_secret()
    );
    wild.posture.migrate().await?;
    trace!(
        "Database migrated at connection {}.",
        wild.posture.introduction().expose_secret()
    );
    info!("Local posture test successful.");
    Ok(())
}

pub async fn bearing(wild: &mut WildGuest) -> Polite<()> {
    local_posture(&wild).await?;
    wild.bearing = wild.bearing.clone()
        .adjust("/book", get(Counsel::book))
        .adjust("/lookup/:id", get(Counsel::lookup))
        .adjust("/lookup_all", get(Counsel::lookup_all))
        .adjust("/check", get(Counsel::check_guest));
    trace!("Bearing: {:#?}", wild.bearing);
    Ok(())
}

pub async fn recall(wild: &WildGuest) -> Recall {
    Recall::from(&wild.posture)
}


