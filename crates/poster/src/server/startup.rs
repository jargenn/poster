use std::time::Duration;

use axum::Router;
use axum::routing::post;
use axum::{Extension, body::Body, http, routing::get};
use eyre::Result;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::LatencyUnit;
use tower_http::trace::{DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{error, info, warn};

use crate::{
    background_jobs::session_maintenance,
    configuration::AppConfig,
    debug::debug_session,
    server::{AppState, endpoints},
};

pub async fn start_server(config: AppConfig) -> Result<()> {
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
    state.warmup_cache().await?;

    let pool = state.pool.clone();
    let auth_data = state.auth_data.clone();
    tokio::task::spawn(async move {
        let mut conn = pool
            .acquire()
            .await
            .expect("Failed to open a connection to the sqlite db");

        session_maintenance(&mut conn, auth_data).await;
        tokio::time::sleep(Duration::from_secs(3600)).await;
    });

    let page_api = Router::new()
        .route("/feed/{page_id}", post(endpoints::page_api::schedule_post))
        .route(
            "/feed/{page_id}/batch",
            post(endpoints::page_api::schedule_multiple_posts),
        )
        .route(
            "/page_credentials",
            get(endpoints::page_api::facebooks_pages),
        )
        .route(
            "/page_credentials/{page_id}",
            get(endpoints::page_api::page_credentials),
        )
        .route(
            "/page_posts/{page_id}",
            get(endpoints::page_api::get_page_posts),
        );

    let facebook = Router::new()
        .nest("/page_api", page_api)
        .route("/{config_id}/oauth/login", get(endpoints::login::fb_login))
        .route(
            "/oauth/callback/{config_id}",
            get(endpoints::login::fb_callback),
        )
        .route("/debug/session/{config_id}", get(debug_session));

    let router = Router::new()
        .nest("/facebook", facebook)
        .route("/config", post(endpoints::config::save))
        .route("/health", get(endpoints::health::health_check))
        .layer(Extension(
            reqwest::ClientBuilder::new()
                .timeout(Duration::from_secs(5))
                .build()?,
        ))
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
