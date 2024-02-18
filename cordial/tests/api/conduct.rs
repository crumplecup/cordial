use cordial::prelude::*;
use crate::prelude::*;
use tracing::info;

#[cfg(feature = "full")]
#[tokio::test]
pub async fn conduct() -> Polite<()> {
    let mut host = Host::from_env().await?;
    info!("Host created.");
    booking(&host).await?;
    info!("Booking successful.");
    guest_check(&mut host).await?;
    info!("Guest check successful.");
    guest_lifecycle(&mut host).await?;
    info!("Guest lifecycle successful.");

    fauxpas()?;
    info!("Fauxpas test successful.");

    Ok(())
}


