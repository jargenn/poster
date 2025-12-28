#[cfg(debug_assertions)]
use std::time::Duration;

use axum::Router;
#[cfg(debug_assertions)]
use axum::{Extension, body::Body, http, routing::get};
use eyre::Result;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use tokio::net::TcpListener;
#[cfg(debug_assertions)]
use tower::ServiceBuilder;
#[cfg(debug_assertions)]
use tower_http::trace::TraceLayer;
use tracing::info;

#[cfg(debug_assertions)]
use crate::server::AppState;
use crate::{
    debug::debug_session,
    login::{fb_callback, fb_login},
};

pub async fn start_server(state: AppState) -> Result<()> {
    // pub async fn start_server(ready: oneshot::Sender<()>) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:3000").await?;
    let url = listener.local_addr()?;

    let manager = SqliteConnectionManager::file(&state.config.database_path);
    let pool = Pool::builder().max_size(10).build(manager)?;

    #[cfg(debug_assertions)]
    let router = Router::new()
        .route("/facebook/login", get(fb_login))
        .route("/facebook/oauth/callback", get(fb_callback))
        .route("/debug/session", get(debug_session))
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
