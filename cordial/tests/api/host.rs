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
    host.bearing = host.bearing.clone()
        .adjust("/book", get(Counsel::book))
        .adjust("/lookup/:id", get(Counsel::lookup))
        .adjust("/lookup_all", get(Counsel::lookup_all))
        .adjust("/check", get(Counsel::check_guest));
    trace!("Bearing: {:#?}", host.bearing);
    Ok(())
}

pub async fn recall(host: &Host) -> Recall {
    Recall::from(&host.posture)
}

pub async fn host() -> Polite<Host> {
    Host::from_env().await
}


