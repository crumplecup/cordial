use cordial::prelude::*;
use crate::prelude::*;
use tracing::info;

#[cfg(feature = "full")]
#[tokio::test]
pub async fn conduct() -> Polite<()> {
    let mut wild = WildGuest::from_env().await?;
    bearing(&mut wild).await?;
    info!("Bearing test successful.");
    create_guest(&wild).await?;
    info!("Guest creation successful.");

    fauxpas()?;
    info!("Fauxpas test successful.");

    Ok(())
}


