//! The `eponym` module contains the [`Host`] struct, with methods for managing [`Guest`] needs.
use crate::prelude::*;
#[cfg(feature = "route")]
use axum::Router;
#[cfg(feature = "route")]
use axum::routing::get;
use tracing::info;
use secrecy::ExposeSecret;
#[cfg(feature = "trace")]
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(feature = "route")]
#[cfg_attr(docsrs, doc(cfg(feature = "route")))]
#[cfg(feature = "trace")]
#[cfg_attr(docsrs, doc(cfg(feature = "trace")))]
#[derive(Debug, Clone)]
pub struct Host {
    pub recall: Recall,
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
        posture.migrate().await?;
        let recall = Recall::from(posture.clone());
        Ok(Self { recall, posture })
    }

    pub fn bearing(&self) -> Router {
        Router::new()
            .route("/book", get(Counsel::book))
            .route("/guests", get(Counsel::lookup_all).post(Counsel::check_in))
            .route("/guests/:id", get(Counsel::lookup).put(Counsel::update).delete(Counsel::check_out))
            .with_state(self.recall.book.clone())
    }

}
