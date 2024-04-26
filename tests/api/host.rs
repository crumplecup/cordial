use cordial::prelude::*;
use secrecy::ExposeSecret;
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
