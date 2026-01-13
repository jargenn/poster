use axum::Router;
use axum::routing::post;
use axum::serve::Serve;
use axum::{Extension, body::Body, http, routing::get};
use facebook_graph_api::auth::Authorized;
use http::{HeaderValue, Method, header};
use moka::future::{Cache, CacheBuilder};
use reqwest::Url;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::task::{AbortHandle, JoinHandle};
use tower::ServiceBuilder;
use tower_http::LatencyUnit;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::{DefaultOnFailure, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tower_sessions::cookie::SameSite;
use tower_sessions::{ExpiredDeletion, Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;
use tracing::{error, info, warn};

use crate::configuration::{DatabaseSettings, FbAppData, MediaSettings};
use crate::{configuration::AppConfig, server::routes};

type Server = Serve<TcpListener, Router, Router>;
pub struct Application {
    port: u16,
    server: Server,
    session_deletion_task: JoinHandle<Result<(), tower_sessions::session_store::Error>>,
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

        let media_settings = config.media_settings;
        let facebook_uri = Url::parse(&config.facebook_uri)?;

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
            facebook_uri,
        };

        let session_store = PostgresStore::new(state.pool.clone());

        session_store.migrate().await?;

        let session_deletion_task = tokio::task::spawn(
            session_store
                .clone()
                .continuously_delete_expired(tokio::time::Duration::from_secs(3600)),
        );

        let session_layer = {
            if cfg!(debug_assertions) {
                SessionManagerLayer::new(session_store)
                    .with_secure(false)
                    .with_same_site(SameSite::Lax)
                    .with_expiry(Expiry::OnInactivity(time::Duration::seconds(10)))
            } else {
                SessionManagerLayer::new(session_store)
                    .with_secure(true)
                    .with_same_site(SameSite::Lax)
                    .with_expiry(Expiry::OnInactivity(time::Duration::seconds(10)))
            }
        };

        let server = run(listener, state, session_layer);

        Ok(Self {
            port,
            server,
            session_deletion_task,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run_until_stopped(self) -> eyre::Result<()> {
        let abort_signal = self.session_deletion_task.abort_handle();
        self.server
            .with_graceful_shutdown(shutdown_signal(abort_signal))
            .await?;

        self.session_deletion_task.await??;

        Ok(())
    }
}

pub fn run(
    listener: TcpListener,
    state: PosterState,
    session_layer: SessionManagerLayer<PostgresStore>,
) -> Server {
    let page_api = Router::new()
        .route(
            "/feed/{page_id}",
            post(routes::page_api::schedule_posts),
        )
        .route(
            "/page_credentials",
            get(routes::page_api::facebooks_pages),
        )
        .route(
            "/page_credentials/{page_id}",
            get(routes::page_api::page_credentials),
        )
        // .route(
        //     "/page_posts/{page_id}",
        //     get(endpoints::page_api::get_page_posts),
        // )
        ;

    let facebook = Router::new()
        .nest("/page_api", page_api)
        .route("/oauth/login", get(routes::fb_login))
        .route("/oauth/callback", get(routes::fb_callback));

    let static_files = ServeDir::new("static").append_index_html_on_directories(true);

    let router = Router::new()
        .nest("/facebook", facebook)
        .route("/config", post(routes::config::save))
        .route("/login", post(routes::login))
        .route("/health", get(routes::health::health_check))
        .fallback_service(static_files)
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

    #[cfg(debug_assertions)]
    let router = router.layer(CorsLayer::very_permissive());

    let router = router.layer(session_layer);

    axum::serve(listener, router)
}

pub fn get_connection_pool(config: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy_with(config.with_db())
}

async fn shutdown_signal(deletion_task_abort_handle: AbortHandle) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { deletion_task_abort_handle.abort() },
        _ = terminate => { deletion_task_abort_handle.abort() },
    }
}

#[derive(Debug, Clone)]
pub struct PosterState {
    pub pool: PgPool,
    pub fb_app_config: Cache<String, FbAppData>,
    pub session_cache: Cache<String, Authorized>,
    pub media_settings: MediaSettings,
    pub facebook_uri: Url,
}
