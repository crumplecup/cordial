use crate::prelude::*;
use cordial::prelude::*;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::test]
pub async fn features() -> Polite<()> {
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
    min_guest()?;
    no_min_guest()?;
    Ok(())
}

#[cfg(feature = "uuid")]
#[cfg(feature = "serial")]
fn min_guest() -> Polite<()> {
    let guest = Guest::new("test", "123");
    info!("Test guest created: {:#?}.", guest);
    Ok(())
}

#[cfg(feature = "uuid")]
fn no_min_guest() -> Polite<()> {
    let guest = Guest::new("test", "123");
    info!("Test guest created: {:#?}.", guest);
    Ok(())
}
