use std::time::Duration;

use eyre::Result;
use poster::{
    db::{cleanup_sessions, create_database},
    server::{AppState, start_server},
    telemetry,
};
use tracing::error;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let state = AppState::new()?;

    #[cfg(debug_assertions)]
    dbg!(&state);

    create_database();
    telemetry::init_tracing();

    tokio::spawn(async move {
        loop {
            cleanup_sessions();
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });

    // let (ready_tx, ready_rx) = oneshot::channel();
    // tokio::spawn(async {
    if let Err(err) = start_server(state).await {
        error!("Server error: {err}");
    }
    // });
    // ready_rx.await?;

    Ok(())
}
