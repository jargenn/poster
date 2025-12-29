use std::time::Duration;

use axum::Router;
use axum::{Extension, body::Body, http, routing::get};
use eyre::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use secrecy::ExposeSecret;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};

use crate::facebook_graph_api::login::{fb_callback, fb_login};
use crate::server::{AppState, endpoints};
use crate::{AppConfig, debug::debug_session};

pub async fn start_server(config: AppConfig) -> Result<()> {
    // pub async fn start_server(ready: oneshot::Sender<()>) -> Result<()> {
    let address = format!("{}:{}", config.host, config.port);
    let listener = match TcpListener::bind(&address).await {
        Ok(listener) => listener,
        Err(err) => {
            warn!("{err}. Trying with another port...");
            match TcpListener::bind(format!("{}:0", config.host)).await {
                Ok(listener) => listener,
                Err(err) => {
                    error!("There aren't available ports, closing application...");
                    return Err(err.into());
                }
            }
        }
    };

    let url = listener.local_addr()?;

    let state = AppState::new(config);
    let manager = SqliteConnectionManager::file(state.config.database_path.expose_secret());
    let pool = Pool::builder().max_size(10).build(manager)?;

    let router = Router::new()
        .route("/facebook/login", get(fb_login))
        .route("/facebook/oauth/callback", get(fb_callback))
        .route("/debug/session", get(debug_session))
        .route(
            "/page_api/page_credentials",
            get(endpoints::page_api::facebooks_pages),
        )
        .layer(Extension(
            reqwest::ClientBuilder::new()
                .timeout(Duration::from_secs(5))
                .build()?,
        ))
        .layer(Extension(pool))
        .layer(
            ServiceBuilder::new().layer(TraceLayer::new_for_http().make_span_with(
                |req: &http::Request<Body>| {
                    use color_eyre::owo_colors::OwoColorize;
                    use tracing::info_span;
                    info_span!("req", method = %req.method().bold(), uri=%req.uri().bold())
                },
            )),
        )
        .with_state(state);

    // let _ = ready.send(());
    info!("Server listening on http://{url}");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("Shutting down");
}
