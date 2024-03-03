use crate::prelude::*;
use cordial::prelude::*;
use tracing::{info, trace};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::test]
pub async fn conduct() -> Polite<()> {
    match tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cordial=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init()
    {
        Ok(_) => {}
        Err(_) => {}
    };
    trace!("Subscriber initialized.");
    let mut host = Host::from_env().await?;
    info!("Host created.");
    booking(&host).await?;
    info!("Booking successful.");
    guest_check(&mut host).await?;
    info!("Guest check successful.");
    guest_lifecycle(&mut host).await?;
    info!("Guest lifecycle successful.");

    info!("Checking improvisation.");
    improvise(&host).await?;
    info!("Improvisation successful.");

    fauxpas()?;
    info!("Fauxpas test successful.");

    Ok(())
}
