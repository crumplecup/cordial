use cordial::prelude::*;
// use axum::routing::{get, post, Router};
use reqwest::Client;
use secrecy::ExposeSecret;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub const LOCAL: &str = "http://localhost:8000";

#[derive(Clone)]
pub struct Voice(reqwest::Client);

impl Voice {
    pub fn new() -> Self {
        let client = Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .cookie_store(true)
                .build()
                .unwrap();
        Voice(client)
    }
}

#[derive(Clone)]
pub struct WildGuest {
    pub recall: Recall,
    pub bearing: Bearing,
    pub posture: Posture,
    pub voice: Voice,
}

impl WildGuest {
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
        let voice = Voice::new();
        Ok(Self { recall, bearing, posture, voice })
    }
}

