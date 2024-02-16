//! The `eponym` module contains the [`Host`] struct, with methods for managing [`Guest`] needs.
use crate::prelude::*;
use tracing::info;
use secrecy::ExposeSecret;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Clone)]
pub struct Host {
    pub recall: Recall,
    pub bearing: Bearing,
    pub posture: Posture,
}

impl Host {
    pub async fn from_env() -> Polite<Self> {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::try_from_default_env()
                  .unwrap_or_else(|_| "cordial=info".into()),
                  )
            .with(tracing_subscriber::fmt::layer())
            .try_init()?;
        let posture = Posture::from_env()?;
        info!(
            "Connection: {}",
            &posture.introduction().expose_secret()
        );
        posture.try_delete().await?;
        posture.create().await?;
        let recall = Recall::from(posture.clone());
        let bearing = Bearing::from(&recall);
        Ok(Self { recall, bearing, posture })
    }

}
