use poster::AppConfig;
use secrecy::ExposeSecret as _;
use std::time::Duration;

use eyre::Result;
use poster::{
    db::{cleanup_sessions, create_database},
    server::start_server,
    telemetry,
};
use tracing::error;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let config_file = std::env::var("CONFIG_FILE").unwrap_or_else(|_| "config".to_owned());
    let config = AppConfig::from_config(config_file.into())?;

    dbg!(&config);

    create_database(config.database_path.expose_secret());
    telemetry::init_tracing();

    tokio::spawn(async move {
        loop {
            cleanup_sessions();
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
