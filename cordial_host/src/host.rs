//! The `host` crate contains the [`Host`] struct, with methods for managing [`Guest`] needs.
use axum::routing::get;
use axum::Router;
use cordial_posture::Posture;
use cordial_recall::Recall;
use counsel::Counsel;
use polite::Polite;
use secrecy::ExposeSecret;
use tracing::info;

#[derive(Debug, Clone)]
pub struct Host {
    pub recall: Recall,
    pub posture: Posture,
}

impl Host {
    pub async fn from_env() -> Polite<Self> {
        let posture = Posture::from_env()?;
        info!("Connection: {}", &posture.introduction().expose_secret());
        posture.try_delete().await?;
        posture.create().await?;
        posture.migrate().await?;
        let recall = Recall::from(posture.clone());
        Ok(Self { recall, posture })
    }

    pub fn bearing(&self) -> Router {
        Router::new()
            .route("/health", get(Counsel::check))
            .route("/book", get(Counsel::book))
            .route("/guests", get(Counsel::lookup_all).post(Counsel::check_in))
            .route(
                "/guests/:id",
                get(Counsel::lookup)
                    .put(Counsel::update)
                    .delete(Counsel::check_out),
            )
            .route("/improv/name", get(Counsel::guest_name))
            .route("/improv/name/num", get(Counsel::guest_name_numbered))
            .route(
                "/improv/pass",
                get(Counsel::guest_pass).post(Counsel::pass_adv),
            )
            // .route("/improv/pass/:length/:numbers/:lowercase/:uppercase/:symbols/:spaces/:exclude/:strict", get(Counsel::pass_adv))
            .with_state(self.recall.book.clone())
    }
}
