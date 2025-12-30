use poster::{AppConfig, background_jobs::session_maintenance};
use secrecy::ExposeSecret as _;
use std::time::Duration;

use eyre::Result;
use poster::{server::start_server, telemetry};
use tracing::error;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    telemetry::init_tracing();

    let config = AppConfig::get_config()?;
    dbg!(&config);

    let db_connection_string = config.database.connection_string();
    let app_secret = config.app_secret.clone();
    let app_id = config.app_id.clone();

    tokio::spawn(async move {
        loop {
            session_maintenance(&db_connection_string, &app_id, app_secret.expose_secret()).await;
            // post_maintenance(db_connection_string, &app_id, app_secret.expose_secret()).await;
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });

    // let (ready_tx, ready_rx) = oneshot::channel();
    // tokio::spawn(async {
    if let Err(err) = start_server(config).await {
        error!("Server error: {err}");
    }
    // });
    // ready_rx.await?;

    Ok(())
}
