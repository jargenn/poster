use std::time::Duration;

use axum::Router;
use axum::routing::post;
use axum::{Extension, body::Body, http, routing::get};
use eyre::Result;
use secrecy::ExposeSecret;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::LatencyUnit;
use tower_http::trace::{DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{error, info, warn};

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

    let state = AppState::new(config.clone());
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_lazy(config.database_path.expose_secret())?;

    let page_api = Router::new()
        .route("/feed/{page_id}", post(endpoints::page_api::post_to_page))
        .route(
            "/page_credentials",
            get(endpoints::page_api::facebooks_pages),
        )
        .route(
            "/page_credentials/{page_id}",
            get(endpoints::page_api::page_credentials),
        );

    let facebook = Router::new()
        .nest("/page_api", page_api)
        .route("/oauth/login", get(endpoints::login::fb_login))
        .route("/oauth/callback", get(endpoints::login::fb_callback))
        .route("/debug/session", get(debug_session));

    let router = Router::new()
        .nest("/facebook", facebook)
        .layer(Extension(
            reqwest::ClientBuilder::new()
                .timeout(Duration::from_secs(5))
                .build()?,
        ))
        .layer(Extension(pool))
        .layer(
            ServiceBuilder::new().layer(
                TraceLayer::new_for_http()
                    .make_span_with(|req: &http::Request<Body>| {
                        use color_eyre::owo_colors::OwoColorize;
                        tracing::info_span!(
                            "http_request",
                            method = %req.method().bold(),
                            uri = %req.uri().bold(),
                            status = tracing::field::Empty,
                            latency_ms = tracing::field::Empty,
                        )
                    })
                    .on_request(DefaultOnRequest::new().level(tracing::Level::DEBUG))
                    .on_response(
                        DefaultOnResponse::new()
                            .level(tracing::Level::INFO)
                            .latency_unit(LatencyUnit::Millis),
                    )
                    .on_failure(
                        DefaultOnFailure::new()
                            .level(tracing::Level::WARN)
                            .latency_unit(LatencyUnit::Millis),
                    ),
            ),
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
