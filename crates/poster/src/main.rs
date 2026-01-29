use eyre::Result;
use poster::configuration::AppConfig;
use poster::server::Application;
use poster::telemetry;
use tracing::error;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let config = AppConfig::get_config()?;
    telemetry::init_tracing(&config);
    dbg!(&config);

    let server = Application::build(config).await?;

    if let Err(err) = server.run_until_stopped().await {
        error!("Server error: {err}");
    }

    Ok(())
}
