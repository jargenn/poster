use std::time::Duration;

use axum::Router;
use axum::routing::post;
use axum::serve::Serve;
use axum::{Extension, body::Body, http, routing::get};
use facebook_graph_api::auth::Authorized;
use moka::future::{Cache, CacheBuilder};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::LatencyUnit;
use tower_http::trace::{DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::{error, warn};

use crate::configuration::{DatabaseSettings, FbAppData, MediaSettings};
use crate::{
    background_jobs::session_maintenance, configuration::AppConfig, debug::debug_session,
    server::endpoints,
};

type Server = Serve<TcpListener, Router, Router>;
pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(config: AppConfig) -> eyre::Result<Self> {
        let connection_pool = get_connection_pool(&config.database);

        let fb_app_config = CacheBuilder::new(100)
            .name(&config.caches.session_data.name)
            .time_to_live(Duration::from_secs(config.caches.session_data.ttl))
            .build();

        let session_cache = CacheBuilder::new(100)
            .name(&config.caches.session_data.name)
            .time_to_live(Duration::from_secs(config.caches.session_data.ttl))
            .build();

        let mut conn = connection_pool.acquire().await?;
        session_maintenance(&mut conn, &session_cache).await;

        let media_settings = config.media_settings;

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
        let port = listener
            .local_addr()
            .expect("Failed to inspect local address of the listener")
            .port();

        let state = PosterState {
            pool: connection_pool,
            fb_app_config,
            session_cache,
            media_settings,
        };

        let server = run(listener, state);
        Ok(Self { port, server })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.with_graceful_shutdown(shutdown_signal()).await
    }
}

pub fn run(listener: TcpListener, state: PosterState) -> Server {
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
        // .route(
        //     "/page_posts/{page_id}",
        //     get(endpoints::page_api::get_page_posts),
        // )
        ;

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
        .with_state(state)
        .layer(Extension(
            reqwest::ClientBuilder::new()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("Failed to build a reqwest client"),
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
        );

    axum::serve(listener, router)
}

pub fn get_connection_pool(config: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy_with(config.with_db())
}

pub async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("Shutting down");
}

#[derive(Debug, Clone)]
pub struct PosterState {
    pub pool: PgPool,
    pub fb_app_config: Cache<String, FbAppData>,
    pub session_cache: Cache<String, Authorized>,
    pub media_settings: MediaSettings,
}
