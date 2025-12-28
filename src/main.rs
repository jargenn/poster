use std::time::Duration;

use axum::{Router, routing::get};
use eyre::Result;
#[cfg(debug_assertions)]
use poster_demo::debug::debug_session;
use poster_demo::{
    AppState,
    db::{cleanup_sessions, create_database},
    login::{fb_callback, fb_login},
};
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt, util::SubscriberInitExt};
use tracing_tree::HierarchicalLayer;

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));

    let layer = HierarchicalLayer::new(2)
        .with_bracketed_fields(true)
        .with_indent_lines(true)
        .with_targets(false);

    Registry::default().with(filter).with(layer).init();
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    create_database();
    init_tracing();

    tokio::spawn(async move {
        loop {
            cleanup_sessions();
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });

    // let (ready_tx, ready_rx) = oneshot::channel();
    // tokio::spawn(async {
    if let Err(err) = start_server().await {
        error!("Server error: {err}");
    }
    // });
    // ready_rx.await?;

    Ok(())
}

pub async fn start_server() -> Result<()> {
    // pub async fn start_server(ready: oneshot::Sender<()>) -> Result<()> {
    let listener = TcpListener::bind("localhost:3000").await?;
    let url = listener.local_addr()?;

    #[cfg(debug_assertions)]
    let router = Router::new()
        .route("/facebook/login", get(fb_login))
        .route("/facebook/oauth/callback", get(fb_callback))
        .route("/debug/session", get(debug_session))
        .with_state(AppState::new());

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
