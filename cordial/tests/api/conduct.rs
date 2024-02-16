use cordial::prelude::*;
use crate::prelude::*;
use tracing::info;

#[cfg(feature = "full")]
#[tokio::test]
pub async fn conduct() -> Polite<()> {
    let mut host = host().await?;
    bearing(&mut host).await?;
    info!("Bearing test successful.");
    create_guest(&host).await?;
    info!("Guest creation successful.");

    fauxpas()?;
    info!("Fauxpas test successful.");

    Ok(())
}


