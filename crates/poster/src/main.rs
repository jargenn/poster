use eyre::Result;
use poster::configuration::AppConfig;
use poster::{server::start_server, telemetry};
use tracing::error;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    telemetry::init_tracing();

    let config = AppConfig::get_config()?;
    dbg!(&config);

    if let Err(err) = start_server(config).await {
        error!("Server error: {err}");
    }

    Ok(())
}
