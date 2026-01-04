use eyre::Result;
use serde::Deserialize;

use crate::configuration::{CacheStore, database::DatabaseSettings, media::MediaSettings};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database: DatabaseSettings,
    pub media_settings: MediaSettings,
    #[serde(rename = "caches")]
    pub caches: CacheStore,
}

impl AppConfig {
    pub fn get_config() -> Result<Self> {
        let base_path = std::env::current_dir().expect("Failed to determine current directory");
        let config_dir = base_path.join("configuration");
        let env = std::env::var("APP_ENVIRONMENT").unwrap_or_else(|_| "local".into());

        let settings = config::Config::builder()
            .add_source(config::File::from(config_dir.join("base")).required(true))
            .add_source(config::File::from(config_dir.join(env.as_str())).required(true))
            .add_source(config::Environment::with_prefix("app").separator("__"))
            .build()?;

        Ok(settings.try_deserialize()?)
    }
}
